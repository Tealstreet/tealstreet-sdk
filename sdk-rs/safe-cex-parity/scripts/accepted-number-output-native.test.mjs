// Compiler-backed source-scan checkpoint, not the full alias-dependency matrix
// or a runtime proof of numeric output. The provider mutation still compiles.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import test from 'node:test';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const sdk = path.resolve(scripts, '../..');
const runner = path.join(scripts, 'accepted-set.mjs');
const scope = 'candle-cache library source scan; alias dependencies and runtime probes excluded';
const marker = 'NUMBER_OUTPUT_SOURCE_CHECKPOINT ';

if (process.argv.includes('--number-output-checkpoint')) {
  let rewrites = 0, checkpoints = 0;
  const stop = 'NUMBER_OUTPUT_CHECKPOINT_COMPLETE';
  const hook = registerHooks({ load(url, context, next) {
    const loaded = next(url, context);
    if (url !== pathToFileURL(runner).href) return loaded;
    const source = typeof loaded.source === 'string' ? loaded.source : Buffer.from(loaded.source).toString('utf8');
    const anchor = '  console.log(`${pkg.name}: ${result.types.filter';
    assert.equal(source.split(anchor).length, 2, 'numeric checkpoint follows the unique unchanged inventory/scan loop');
    rewrites++;
    const injection = `
  if (pkg.name === 'safe-cex-candle-cache' && target.name === 'safe_cex_candle_cache') {
    assert.ok(macroOrigins.includes('safe_cex_num::js_number_output'), 'numeric checkpoint requires compiler-verified provider');
    assert.ok(result.types.some(owner => owner.name.endsWith('::CandleCacheChunkRecord') && owner.deserialize), 'numeric checkpoint requires real compiler decoder owner');
    const consumer = fs.readFileSync(path.join(root, 'safe-cex-candle-cache/src/types.rs'), 'utf8');
    assert.equal(consumer.split('safe_cex_num::js_number_output!(self, struct { candles, last_accessed });').length, 2, 'numeric checkpoint must inspect the actual registration');
    console.log(${JSON.stringify(marker)} + JSON.stringify({ scope: ${JSON.stringify(scope)}, package: pkg.name, target: target.name }));
    throw Object.assign(new Error(${JSON.stringify(stop)}), { code: ${JSON.stringify(stop)} });
  }
`;
    return { ...loaded, source: source.replace(anchor, injection + anchor) };
  } });
  process.argv = [process.execPath, runner, '--inventory', '--package=safe-cex-candle-cache'];
  try { await import(pathToFileURL(runner).href); }
  catch (error) { if (error.code === stop) checkpoints++; else throw error; }
  finally { hook.deregister(); }
  assert.equal(rewrites, 1, 'numeric checkpoint must use the maintained driver');
  assert.equal(checkpoints, 1, 'numeric checkpoint must finish the real library scan');
} else {
  test('numeric production compiler binding catches a compiling recipe mutation', { timeout: 1200000 }, () => {
    const execute = () => {
      const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--number-output-checkpoint'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      const output = `${child.stdout}\n${child.stderr}`;
      assert.doesNotMatch(output, /cargo .*did not complete|could not compile|error\[E\d+\]/, 'compiler failure is not a numeric recipe control');
      return { ...child, output };
    };
    const positive = () => {
      const child = execute();
      assert.equal(child.status, 0, child.output);
      const receipts = child.output.split('\n').filter(line => line.startsWith(marker)).map(line => JSON.parse(line.slice(marker.length)));
      assert.deepEqual(receipts, [{ scope, package: 'safe-cex-candle-cache', target: 'safe_cex_candle_cache' }], 'one actual compiler-backed library checkpoint');
    };
    const provider = path.join(sdk, 'safe-cex-num/src/numeric_ingress/eligibility.rs');
    const original = fs.readFileSync(provider, 'utf8');
    const before = 'let Self { $($field,)* $($opaque),+ }';
    assert.equal(original.split(before).length, 2, 'native recipe mutation selects exactly one reviewed arm');
    positive();
    try {
      fs.writeFileSync(provider, original.replace(before, `let _numeric_recipe_control = (); ${before}`));
      const child = execute();
      assert.equal(child.status, 1, child.output);
      assert.match(child.output, /NUMBER_OUTPUT_PROVIDER assertion: recipe changed; expression-macro review required/);
      assert.ok(!child.output.includes(marker), 'changed recipe must refuse before source-scan success');
      console.log('numeric production recipe mutation: named assertion; process exit 1');
    } finally { fs.writeFileSync(provider, original); }
    positive();
  });
}
