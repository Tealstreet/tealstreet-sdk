import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

test('authored JSON order detects removal of actual Aster query key sorting', () => {
  const workspace = fileURLToPath(new URL('../..', import.meta.url));
  const source = path.join(workspace, 'safe-cex-venue-asterdex/src/api_helpers.rs');
  const original = fs.readFileSync(source);
  const anchor = 'for key in js_own_keys(params.keys().map(String::as_str)) {';
  assert.equal(original.toString().split(anchor).length, 2, 'one production key-ordering loop');
  const logs = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-json-native-'));
  const run = label => {
    const result = spawnSync(process.execPath, ['safe-cex-venue-asterdex/scripts/accepted-helper-contracts.mjs'], {
      cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, RUSTC_BOOTSTRAP: '1', SAFE_CEX_TYPESCRIPT_PATH: process.env.SAFE_CEX_TYPESCRIPT_PATH ?? path.join(workspace, '../node_modules/typescript') },
    });
    fs.writeFileSync(path.join(logs, `${label}.log`), result.stdout + result.stderr);
    return result;
  };
  try {
    const baseline = run('baseline');
    assert.equal(baseline.status, 0, baseline.stdout + baseline.stderr);
    fs.writeFileSync(source, original.toString().replace(anchor, 'for key in params.keys().map(String::as_str) {').replace('JsCoercionError, js_own_keys,', 'JsCoercionError,'));
    const mutant = run('removed-key-sorting');
    const output = mutant.stdout + mutant.stderr;
    assert.equal(mutant.status, 1, output);
    assert.match(output, /function output mismatch/, 'native order mutation must fail an executed output assertion');
    assert.match(output, /build_(?:aster_dex_code_)?query_string/, 'native order mutation must name the real production owner');
    assert.doesNotMatch(output, /could not compile|error\[E\d+\]/, 'compiler errors are not runtime controls');
    console.log('authored JSON native key ordering: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(source, original);
  }
  const restored = run('restored');
  assert.equal(restored.status, 0, restored.stdout + restored.stderr);
  assert.deepEqual(fs.readFileSync(source), original, 'exact production source restoration');
});
