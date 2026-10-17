import assert from 'node:assert/strict';
import test from 'node:test';
import { nodeProbeConfiguration, executeNodeProbes, nodeProbeHarness } from './accepted-node-probes.mjs';

const pkg = { package: 'safe-cex-ffi', packageId: 'actual-ffi' };
const target = { name: 'safe_cex_ffi', crate_types: ['cdylib', 'rlib'] };
const configuration = { target: 'native', features: ['node'] };

test('real Node probe execution is restricted to the production FFI addon profile', () => {
  assert.equal(nodeProbeConfiguration(pkg, target, configuration), true);
  assert.equal(nodeProbeConfiguration({ package: 'other' }, target, configuration), false);
  assert.equal(nodeProbeConfiguration(pkg, target, { target: 'native', features: [] }), false);
  for (const change of [
    row => { row.configuration.target = 'wasm32-unknown-unknown'; },
    row => { row.configuration.features.push('wasm'); },
    row => { row.target.name = 'other'; },
    row => { row.target.crate_types = ['rlib']; },
  ]) {
    const row = structuredClone({ pkg, target, configuration }); change(row);
    assert.throws(() => nodeProbeConfiguration(row.pkg, row.target, row.configuration), /accepted-set:/);
  }
  assert.match(nodeProbeHarness(), /pub fn accepted_set_native_run\(env: napi::Env\)/);
  assert.match(nodeProbeHarness(), /__accepted_set_run\(\)/);
  assert.match(nodeProbeHarness(), /value\.to_bits\(\)/);
});

test('Node probe execution refuses failed builds and foreign or ambiguous Cargo artifacts', () => {
  assert.throws(() => executeNodeProbes({ status: 1, stdout: '', stderr: 'compiler failure' }, pkg, '/tmp', '/actual/Cargo.toml'), /addon compilation failed/);
  const artifact = { reason: 'compiler-artifact', package_id: 'actual-ffi', manifest_path: '/actual/Cargo.toml', target, filenames: [] };
  for (const rows of [[], [{ ...artifact, manifest_path: '/foreign/Cargo.toml' }], [artifact, artifact]]) {
    assert.throws(() => executeNodeProbes({ status: 0, stdout: rows.map(row => JSON.stringify(row)).join('\n'), stderr: '' }, pkg, '/tmp', '/actual/Cargo.toml'), /one actual FFI Cargo artifact/);
  }
  for (const filenames of [[], ['one.dylib', 'two.so']]) {
    assert.throws(() => executeNodeProbes({ status: 0, stdout: JSON.stringify({ ...artifact, filenames }), stderr: '' }, pkg, '/tmp', '/actual/Cargo.toml'), /one actual Node dynamic library/);
  }
});
