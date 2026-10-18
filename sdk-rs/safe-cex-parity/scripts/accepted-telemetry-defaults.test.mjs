import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { runProbes } from './accepted-probes.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sourcePath = path.join(root, 'safe-cex-types/src/telemetry.rs');
const original = fs.readFileSync(sourcePath, 'utf8');
const traitName = 'safe_cex_types::telemetry::TelemetrySink';
const classifications = loadPolicy('default-traits');
function cargo(args, env = {}) {
  const result = spawnSync('cargo', ['--color', 'never', ...args], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_BUILD_JOBS: '2', ...env } });
  assert.equal(result.status, 0, `telemetry preparation failed; compiler failures are not controls\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
function probe(reviewedSourceSha) {
  const metadata = JSON.parse(cargo(['metadata', '--no-deps', '--locked', '--format-version', '1']));
  const member = metadata.packages.find(item => item.name === 'safe-cex-types');
  assert.ok(member && metadata.workspace_members.includes(member.id));
  const target = member.targets.find(item => item.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', member.name, '--lib', '--jobs', '2', '--', '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', 'safe_cex_types.json')));
  const reviewed = structuredClone(classifications);
  if (reviewedSourceSha) reviewed[traitName].sourceSha256 = reviewedSourceSha;
  const found = inventory(doc, { onlyDefaultTraits: [traitName], defaultTraits: reviewed, readSource: file => fs.readFileSync(path.resolve(root, file)) });
  assert.deepEqual(found.scope, { onlyDefaultTraits: [traitName] });
  assert.deepEqual(found.defaultWitnesses[0].methods.map(method => method.name), ['capture_exception', 'event']);
  const receipts = runProbes(root, metadata, [{ package: member.name, target, ...found, features: [], locals: [] }], {});
  assert.deepEqual(receipts.map(row => row.name), [traitName]);
  assert.equal(receipts[0].classificationOnly, false, 'defaults must execute rather than receive a non-decoder skip');
  assert.deepEqual(receipts[0].failures, [], JSON.stringify(receipts));
}
function child(reviewedSourceSha) {
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--probe', ...(reviewedSourceSha ? [reviewedSourceSha] : [])], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  assert.ok(!result.error, String(result.error));
  const output = `${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output, /compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  return { ...result, output };
}
if (process.argv[2] === '--probe') {
  probe(process.argv[3]);
} else {
  test('telemetry defaults execute inherited native results and reject body drift', { timeout: 1200000 }, () => {
    const baseline = child();
    assert.equal(baseline.status, 0, baseline.output);
    const cases = [
      ['telemetry default discarded decoder source pin', value => value.replace('        Ok(())', '        let _ = serde_json::from_str::<serde_json::Value>(_name); Ok(())'), false, /reviewed default-method source changed/],
      ['telemetry default native error', value => value.replace('        Ok(())', '        Err(_data)'), true, /function output mismatch/],
    ];
    for (const [name, mutate, refreshControlHash, expected] of cases) {
      try {
        const changed = mutate(original);
        assert.notEqual(changed, original, `${name}: mutation applied`);
        fs.writeFileSync(sourcePath, changed);
        // Only the native-error control bypasses its in-memory source hash:
        // independently fixed native outputs must also detect that mutation.
        const result = child(refreshControlHash ? createHash('sha256').update(changed).digest('hex') : undefined);
        assert.equal(result.status, 1, result.output);
        assert.match(result.output, /AssertionError/);
        assert.match(result.output, expected);
        console.log(`${name}: named assertion; process exit 1`);
      } finally { fs.writeFileSync(sourcePath, original); }
      const restored = child();
      assert.equal(restored.status, 0, restored.output);
    }
  });
}
