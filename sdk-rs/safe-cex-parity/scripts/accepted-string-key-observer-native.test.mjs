import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { loadContracts } from './accepted-contract-files.mjs';
import { scopedMeasurements } from './accepted-measure-scope.mjs';
import { runProbes } from './accepted-probes.mjs';

const workspace = fileURLToPath(new URL('../..', import.meta.url));
const owner = 'safe_cex_num::json_stringify::JsJsonValue';

test('generated JsJsonValue observer compiles and observes native String object keys', { timeout: 1200000 }, () => {
  const run = () => {
    const result = spawnSync(process.execPath, [
      'safe-cex-parity/scripts/accepted-set.mjs', '--measure', '--jobs=2',
      '--package=safe-cex-num', `--owner=${owner}`,
    ], {
      cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, CARGO_INCREMENTAL: '0' },
    });
    const log = result.stdout + result.stderr;
    assert.equal(result.status, 0, `String-key observer must compile and execute through production From<Value>:\n${log}`);
    const target = path.resolve(workspace, process.env.CARGO_TARGET_DIR ?? 'target');
    const results = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-results.json'), 'utf8'));
    assert.equal(results.length, 1, 'only the selected production owner is measured');
    assert.equal(results[0].name, owner);
    assert.equal(results[0].classificationOnly, false, 'a classification cannot replace native observations');
    return results[0].failures;
  };
  assert.deepEqual(run(), [], 'all authored native payload and object-key observations must match');
  // Reuse the authenticated compiler inventory for observer-only controls. The
  // production source stays unchanged; runProbes makes a fresh instrumented copy.
  const prepared = spawnSync('cargo', ['metadata', '--locked', '--format-version=1'], {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_INCREMENTAL: '0' },
  });
  assert.equal(prepared.status, 0, prepared.stderr);
  const metadata = JSON.parse(prepared.stdout);
  const packages = scopedMeasurements(JSON.parse(fs.readFileSync(
    path.join(metadata.target_directory, 'accepted-set-inventory.json'), 'utf8')),
  { package: 'safe-cex-num', modules: [], owners: [owner] }, workspace);
  const generated = contracts => {
    const results = runProbes(workspace, metadata, packages, contracts, { target: 'native', features: [], jobs: 2 });
    assert.equal(results.length, 1);
    assert.equal(results[0].name, owner);
    assert.equal(results[0].classificationOnly, false);
    return results[0].failures;
  };
  const contracts = loadContracts();
  const anchor = '[k,observe(v)]';
  assert.equal(contracts[owner].call.split(anchor).length, 2, 'one native String-key observation');
  contracts[owner].call = contracts[owner].call.replace(anchor, '[{ let _ = k; "wrong key" },observe(v)]');
  const failures = generated(contracts);
  assert.equal(failures.length, 2, 'both ASCII and Unicode object fixtures reject wrong keys');
  assert.ok(failures.some(failure => failure.includes('function output mismatch') && failure.includes('wrong key')),
    'a compiled wrong-key observer must fail the real native output assertion');
  const assertion = spawnSync(process.execPath, ['--input-type=module', '--eval',
    `import assert from 'node:assert/strict'; assert.deepEqual(${JSON.stringify(failures)}, [], 'native String-key observer must match production keys');`,
  ], { encoding: 'utf8' });
  assert.equal(assertion.status, 1, assertion.stderr);
  assert.match(assertion.stderr, /native String-key observer must match production keys/);
  console.log('native String-key observer corruption: named assertion; process exit 1');
  assert.deepEqual(generated(loadContracts()), [], 'unchanged production observer restores GREEN');
});
