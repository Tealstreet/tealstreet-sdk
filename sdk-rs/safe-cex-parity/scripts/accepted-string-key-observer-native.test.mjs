import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { loadContracts, contractJson } from './accepted-contract-files.mjs';

const workspace = fileURLToPath(new URL('../..', import.meta.url));
const owner = 'safe_cex_num::json_stringify::JsJsonValue';

test('generated JsJsonValue observer compiles and observes native String object keys', { timeout: 1200000 }, () => {
  const run = (extra = []) => {
    const result = spawnSync(process.execPath, [
      'safe-cex-parity/scripts/accepted-set.mjs', '--measure', '--jobs=2',
      '--package=safe-cex-num', `--owner=${owner}`, ...extra,
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
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-string-key-'));
  try {
    const contracts = loadContracts();
    const anchor = '[k,observe(v)]';
    assert.equal(contracts[owner].call.split(anchor).length, 2, 'one native String-key observation');
    contracts[owner].call = contracts[owner].call.replace(anchor, '[{ let _ = k; "wrong key" },observe(v)]');
    const file = path.join(directory, 'wrong-key.json');
    fs.writeFileSync(file, contractJson(contracts));
    const failures = run([`--contracts=${file}`]);
    assert.equal(failures.length, 2, 'both ASCII and Unicode object fixtures reject wrong keys');
    assert.ok(failures.some(failure => failure.includes('function output mismatch') && failure.includes('wrong key')),
      'a compiled wrong-key observer must fail the real native output assertion');
    const assertion = spawnSync(process.execPath, ['--input-type=module', '--eval',
      `import assert from 'node:assert/strict'; assert.deepEqual(${JSON.stringify(failures)}, [], 'native String-key observer must match production keys');`,
    ], { encoding: 'utf8' });
    assert.equal(assertion.status, 1, assertion.stderr);
    assert.match(assertion.stderr, /native String-key observer must match production keys/);
    console.log('native String-key observer corruption: named assertion; process exit 1');
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
  assert.deepEqual(run(), [], 'unchanged production observer restores GREEN');
});
