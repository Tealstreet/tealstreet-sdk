import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import { loadContracts } from './accepted-contract-files.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { runProbes } from './accepted-probes.mjs';
import { reviewedLocalHelper } from './accepted-local-helpers.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sourcePath = path.join(root, 'safe-cex-types/src/ws_auth.rs');
const original = fs.readFileSync(sourcePath, 'utf8');
const helperLabel = 'safe-cex-types/src/ws_auth.rs::Wire';
const ownerName = 'safe_cex_types::ws_auth::WsAuthResponse';
const helpers = loadPolicy('local-type-helpers');
const contracts = loadContracts();
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', ['--color', 'never', ...args], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_BUILD_JOBS: '2', ...extraEnv } });
  assert.equal(result.status, 0, `WS helper preparation failed; compiler failures are not controls\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
function probe(reviewedHelpers = helpers) {
  const metadata = JSON.parse(cargo(['metadata', '--no-deps', '--locked', '--format-version', '1']));
  const member = metadata.packages.find(item => item.name === 'safe-cex-types');
  assert.ok(member && metadata.workspace_members.includes(member.id));
  const target = member.targets.find(item => item.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', member.name, '--lib', '--jobs', '2', '--', '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
  const found = inventory(JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', 'safe_cex_types.json'))), { onlyOwners: [ownerName] });
  const owner = found.types.find(item => item.name === ownerName);
  assert.ok(owner?.deserialize, 'WS helper must execute the actual compiler-inventoried parent');
  assert.equal(path.resolve(root, owner.span.filename), sourcePath);
  assert.equal(reviewedLocalHelper({ label: helperLabel, line: 170, column: 9 }, reviewedHelpers, fs.readFileSync(sourcePath), [owner], contracts), true);
  return runProbes(root, metadata, [{ package: member.name, target, types: [owner], locals: [], delegatedLocals: [{ name: helperLabel, runtimeOwner: ownerName }] }], contracts);
}
function passed(results) {
  assert.deepEqual(results.map(row => row.name), [ownerName, helperLabel]);
  assert.ok(results.every(row => row.failures.length === 0), JSON.stringify(results));
  assert.equal(results[1].classificationOnly, true);
  assert.equal(results[1].category, 'private-decoder-helper');
}
function child(reviewedSourceSha) {
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--probe', ...(reviewedSourceSha ? [reviewedSourceSha] : [])], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  assert.ok(!result.error, String(result.error));
  const output = `${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output, /compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  return { ...result, output };
}
if (process.argv[2] === '--probe') {
  const reviewed = structuredClone(helpers);
  if (process.argv[3]) reviewed[helperLabel].sourceSha256 = process.argv[3];
  passed(probe(reviewed));
} else {
  test('WS auth helper uses actual parent native fields and detects swap and null loss', { timeout: 1200000 }, () => {
    const baseline = child();
    assert.equal(baseline.status, 0, baseline.output);
    const mutations = [
      ['WS helper native field swap', value => value.replace('ws_auth_signature: wire.ws_auth_signature,\n                    error: wire.error,', 'ws_auth_signature: wire.error,\n                    error: wire.ws_auth_signature,')],
      ['WS helper native null loss', value => value.replace('id: wire.id,', 'id: wire.id.filter(|value| !value.is_null()),')],
    ];
    for (const [name, mutate] of mutations) {
      try {
        const changed = mutate(original);
        assert.notEqual(changed, original, `${name}: mutation applied`);
        fs.writeFileSync(sourcePath, changed);
        assert.throws(() => reviewedLocalHelper({ label: helperLabel, line: 170, column: 9 }, helpers, Buffer.from(changed), [{ name: ownerName, deserialize: true }], contracts), /reviewed helper source changed/);
        // Refresh only the control child's in-memory hash so the independently
        // pinned native observations must also catch the source mutation.
        const result = child(createHash('sha256').update(changed).digest('hex'));
        assert.equal(result.status, 1, `${name}: ${result.output}`);
        assert.match(result.output, /AssertionError/);
        assert.match(result.output, /Deserialize output mismatch/);
        console.log(`${name}: named assertion; process exit ${result.status}`);
      } finally { fs.writeFileSync(sourcePath, original); }
      const restored = child();
      assert.equal(restored.status, 0, restored.output);
    }
  });
}
