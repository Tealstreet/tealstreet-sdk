// This checkpoint verifies the actual SDK alias/compiler claim, before later
// source and runtime probes. The ordinary matrix separately runs the full emitter.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { registerHooks } from 'node:module';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
const scripts = path.dirname(fileURLToPath(import.meta.url));
const runner = path.join(scripts, 'accepted-set.mjs');
const router = path.join(scripts, 'accepted-callable-alias.mjs');
const owner = 'safe_cex_crypto::poseidon_loaders::LoaderFuture';
const marker = 'LOADER_FUTURE_COMPILER ';
if (process.argv.includes('--loader-future-checkpoint')) {
  const sentinel = {};
  const mutation = process.argv.includes('--remove-loader-coverage');
  let checkpoints = 0, rewrites = 0, removed = 0;
  globalThis.__loaderFutureCompiler = (alias, probe, pkg, target) => {
    assert.equal(pkg.name, 'safe-cex-crypto');
    assert.equal(target.name, 'safe_cex_crypto');
    assert.equal(alias.claim.rs, 'safe-cex-crypto/src/poseidon_loaders.rs::LoaderFuture');
    assert.equal(probe.suffix, '<::serde_json::Value>');
    assert.equal(probe.expectedType, '::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output=::std::result::Result<::serde_json::Value,crate::poseidon_loaders::LoaderError>>>>');
    console.log(marker + JSON.stringify({ owner: alias.name, suffix: probe.suffix }));
    throw sentinel;
  };
  const hook = registerHooks({ load(url, context, next) {
    const loaded = next(url, context);
    const source = typeof loaded.source === 'string' ? loaded.source : Buffer.from(loaded.source ?? '').toString('utf8');
    if (url === pathToFileURL(router).href && mutation) {
      const anchor = '  const loader = loaderFutureAliasProbe(alias, contract, context);';
      assert.equal(source.split(anchor).length, 2, 'unique maintained monomorphization route');
      removed++;
      return { ...loaded, source: source.replace(anchor, '  const loader = undefined;') };
    }
    if (url !== pathToFileURL(runner).href) return loaded;
    const anchor = '\n      alias.claim = aliasClaim(alias, claimContext, contract);';
    assert.equal(source.split(anchor).length, 2, 'unique ordinary production alias claim');
    rewrites++;
    return { ...loaded, source: source.replace(anchor, anchor + `
      if (alias.name === ${JSON.stringify(owner)}) globalThis.__loaderFutureCompiler(alias, genericAliasProbe(alias, contract, aliasCompilerContext), pkg, target);`) };
  } });
  process.argv = [process.execPath, runner, '--inventory', '--package=safe-cex-crypto'];
  try { await import(pathToFileURL(runner).href); assert.fail('loader compiler checkpoint must stop explicitly'); }
  catch (error) { if (error === sentinel) checkpoints++; else throw error; }
  finally { hook.deregister(); delete globalThis.__loaderFutureCompiler; }
  assert.equal(rewrites, 1);
  assert.equal(removed, mutation ? 1 : 0);
  assert.equal(checkpoints, 1);
} else {
  test('loader future actual compiler coverage refuses removal and restores the named assertion', { timeout: 1200000 }, () => {
    const original = fs.readFileSync(router);
    const execute = mutation => {
      const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--loader-future-checkpoint', ...(mutation ? ['--remove-loader-coverage'] : [])], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      const output = `${child.stdout}\n${child.stderr}`;
      assert.doesNotMatch(output, /cargo .*did not complete|could not compile|error\[E\d+\]/, 'compiler failure is not a monomorphization control');
      assert.equal(child.status, mutation ? 1 : 0, output);
      if (mutation) {
        assert.match(output, /LoaderFuture: accepted-set assertion: generic alias requires explicit compiler monomorphization coverage: source must directly name its type parameter/);
        assert.ok(!output.includes(marker), 'missing coverage refuses before compiler receipt');
      } else {
        const receipts = output.split('\n').filter(line => line.startsWith(marker)).map(line => JSON.parse(line.slice(marker.length)));
        assert.deepEqual(receipts, [{ owner, suffix: '<::serde_json::Value>' }]);
      }
      assert.deepEqual(fs.readFileSync(router), original, 'native control never modifies maintained source bytes');
    };
    execute(false);
    execute(true);
    console.log('loader future removed monomorphization coverage: named assertion; process exit 1');
    execute(false);
  });
}
