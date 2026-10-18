// Compiler-backed dependency source scan, not a numeric runtime-output proof.
// Both negative controls finish compilation before the maintained scan refuses.
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
const marker = 'NUMBER_OUTPUT_EXPORT_CHECKPOINT ';
const compiled = 'NUMBER_OUTPUT_EXPORT_COMPILER_COMPLETE';

if (process.argv.includes('--number-output-export-checkpoint')) {
  const stop = 'NUMBER_OUTPUT_EXPORT_CHECKPOINT_COMPLETE';
  const removeOrigin = process.argv.includes('--remove-export-origin');
  let rewrites = 0, checkpoints = 0;
  const hook = registerHooks({ load(url, context, next) {
    const loaded = next(url, context);
    if (url !== pathToFileURL(runner).href) return loaded;
    let source = String(loaded.source);
    const anchor = '  // Same-name block-local types in different bodies are distinct declarations;';
    assert.equal(source.split(anchor).length, 2, 'checkpoint follows the full unchanged dependency effect scan');
    if (removeOrigin) {
      const origin = 'dependencyMacroOrigins.push(...verifiedNumberOutputOrigin(dependencyDoc, dependency, metadata, providerArtifacts, scanner));';
      assert.equal(source.split(origin).length, 2, 'control removes only the dependency numeric origin');
      source = source.replace(origin, 'dependencyMacroOrigins.push(...[]);');
    }
    const compilerAnchor = '  const localSources = ';
    assert.equal(source.split(compilerAnchor).length, 2, 'compiler receipt follows the actual consumer check');
    source = source.replace(compilerAnchor, `  console.log(${JSON.stringify(compiled)});\n` + compilerAnchor);
    rewrites++;
    return { ...loaded, source: source.replace(anchor, `
  if (pkg.name === 'safe-cex' && target.name === 'safe_cex') {
    const providers = [...globProviders].filter(provider => provider.sources.some(file => file.endsWith('/safe-cex-exchange/src/cross_trade/types.rs')));
    assert.equal(providers.length, 1, 'actual exchange dependency export provider required');
    assert.ok(providers[0].effectsChecked, 'complete dependency effect scan must finish');
    assert.ok(providers[0].macroOrigins.includes('safe_cex_num::js_number_output'), 'actual dependency numeric compiler origin required');
    console.log(${JSON.stringify(marker)} + JSON.stringify({ package: pkg.name, provider: 'safe-cex-exchange', origin: 'safe_cex_num::js_number_output' }));
    throw Object.assign(new Error(${JSON.stringify(stop)}), { code: ${JSON.stringify(stop)} });
  }
` + anchor) };
  } });
  process.argv = [process.execPath, runner, '--inventory', '--package=safe-cex', '--jobs=1'];
  try { await import(pathToFileURL(runner).href); }
  catch (error) { if (error.code === stop) checkpoints++; else throw error; }
  finally { hook.deregister(); }
  assert.equal(rewrites, 1, 'checkpoint must execute the maintained driver');
  assert.equal(checkpoints, 1, 'checkpoint must finish the actual dependency scan');
} else {
  test('numeric dependency exports authenticate the real compiler origin and reject compiling grammar changes', { timeout: 1200000 }, () => {
    const execute = (args = []) => {
      const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--number-output-export-checkpoint', ...args], {
        cwd: sdk, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, CARGO_TARGET_DIR: path.join(sdk, 'target/accepted-inventory'), CARGO_INCREMENTAL: '0', RUSTC_BOOTSTRAP: '1' },
      });
      const output = `${child.stdout}\n${child.stderr}`;
      assert.ok(!child.error, output);
      assert.doesNotMatch(output, /cargo .*did not complete|could not compile|error\[E\d+\]/, 'compile errors are not numeric export controls');
      return { ...child, output };
    };
    const positive = () => {
      const child = execute();
      assert.equal(child.status, 0, child.output);
      const receipts = child.output.split('\n').filter(line => line.startsWith(marker)).map(line => JSON.parse(line.slice(marker.length)));
      assert.deepEqual(receipts, [{ package: 'safe-cex', provider: 'safe-cex-exchange', origin: 'safe_cex_num::js_number_output' }]);
    };
    const refusal = (child, expected) => {
      assert.equal(child.status, 1, child.output);
      assert.ok(child.output.includes(compiled), 'negative control must finish the actual consumer compilation');
      assert.match(child.output, expected);
      assert.ok(!child.output.includes(marker), 'unverified export must refuse before success');
    };
    positive();
    refusal(execute(['--remove-export-origin']), /cross_trade\/types\.rs::UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR.*dependency export source effect is unverified/);
    console.log('numeric dependency export missing origin: named assertion; process exit 1');
    const file = path.join(sdk, 'safe-cex-exchange/src/cross_trade/types.rs');
    const original = fs.readFileSync(file, 'utf8');
    const anchor = 'js_number_output!(self, enum { Balance, Absolute });';
    assert.equal(original.split(anchor).length, 2, 'grammar control selects one actual production macro call');
    try {
      fs.writeFileSync(file, original.replace(anchor, 'js_number_output!((self), enum { Balance, Absolute });'));
      refusal(execute(), /dependency export module discovery assertion:.*DOC_CONFIGURATION/s);
      console.log('numeric dependency export opaque operand: named assertion; process exit 1');
    } finally { fs.writeFileSync(file, original); }
    positive();
    assert.equal(fs.readFileSync(file, 'utf8'), original);
  });
}
