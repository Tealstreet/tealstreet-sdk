import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { parseManifest, serializeManifest } = require('../extract-ts/src/manifest.ts');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const renames = {
  'cross-failure-key': 'CrossTradeManager#_failureKey',
  'cross-record-success': 'CrossTradeManager#_recordSuccess',
  'cross-meets-minimum': 'CrossTradeManager#_meetsMinAmount',
};

function control(name) {
  assert.ok(Object.hasOwn(renames, name) || name === 'backpack-provenance', 'Unknown manifest repair control');
  const backpack = name === 'backpack-provenance';
  const relative = backpack ? 'exchanges/backpack/backpack.api.ts.toml' : 'crossTrade/runtime/CrossTradeManager.ts.toml';
  const source = backpack ? 'packages/safe-cex/src/exchanges/backpack/backpack.api.ts' : 'packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.ts';
  const file = path.join(root, 'sdk-rs/parity/manifest', relative);
  // Compare with HEAD so staged input cannot inherit the commit's attribution.
  const clean = spawnSync('git', ['diff', '--quiet', 'HEAD', '--', file], { cwd: root });
  assert.ifError(clean.error);
  assert.equal(clean.status, 0, 'Control requires manifest identical to HEAD');
  const sha = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
  const original = readFileSync(file);
  const data = parseManifest(original.toString('utf8'), source, file);
  const nativeAssertion = backpack ? 'BACKPACK native source declarations' : `CROSSTRADE explicit rename token: ${renames[name]}`;
  if (backpack) {
    assert.equal(data.file.rust_files.length, 2);
    data.file.rust_files = data.file.rust_files.filter(file => file !== 'safe-cex-venue-backpack/src/api_factory.rs');
    assert.equal(data.file.rust_files.length, 1);
  } else {
    const row = data.symbol.find(row => row.ts === renames[name] && row.kind === 'method');
    assert.ok(row && typeof row.rs === 'string' && row.note.includes(row.rs));
    row.note = row.note.replace(row.rs, '[removed-for-control]');
    assert.ok(!row.note.includes(row.rs));
  }
  const run = phase => {
    const result = spawnSync(process.execPath, [
      '--import', 'tsx', '--test', '--test-reporter=tap', 'sdk-rs/parity/scripts/manifest-repairs.test.mjs',
    ], { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    assert.ifError(result.error);
    const output = result.stdout + result.stderr;
    writeFileSync(path.join(root, 'sdk-rs/target', `manifest-control-${name}-${phase}.log`), output);
    return { ...result, output };
  };
  let broken;
  let failure;
  try {
    writeFileSync(file, serializeManifest(data));
    broken = run('mutated');
    assert.equal(broken.status, 1, 'Mutation must fail the test target');
    assert.match(broken.output, /^# tests 3$/m, 'All three focused tests must execute');
    assert.ok(broken.output.includes(nativeAssertion), 'Native declaration assertion must catch mutation');
    assert.ok(broken.output.includes('REPAIR selected source declarations validate'), 'Maintained checker assertion must catch mutation');
  } catch (error) {
    failure = error;
  } finally {
    writeFileSync(file, original);
  }
  const restored = run('restored');
  assert.equal(restored.status, 0, 'Restored target must pass');
  assert.match(restored.output, /^# tests 3$/m);
  assert.match(restored.output, /^# fail 0$/m);
  assert.deepEqual(readFileSync(file), original, 'Exact manifest bytes must be restored');
  if (failure) throw failure;
  console.log(JSON.stringify({ control: name, sha, mutated: broken.status, restored: restored.status, nativeAssertion, checkerAssertion: 'REPAIR selected source declarations validate', exactBytesRestored: true }));
}

try {
  assert.equal(process.argv.length, 3, 'Usage: node --import tsx manifest-repair-controls.mjs <control>');
  control(process.argv[2]);
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
