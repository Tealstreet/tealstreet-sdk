// Seven individually reviewed response-shape outputs and typed conversion. Not the whole-venue accepted gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env },
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const selected = ['safe_cex_venue_binance::response_shape::BinanceResponseError', 'safe_cex_venue_binance::response_shape::describe', 'safe_cex_venue_binance::response_shape::describe_binance_pm_response_shape', 'safe_cex_venue_binance::response_shape::describe_binance_response_shape', 'safe_cex_venue_binance::response_shape::native_malformed', 'safe_cex_venue_binance::response_shape::require_binance_array_response', 'safe_cex_venue_binance::response_shape::require_binance_pm_array_response'];
const prefix = '--expect-output-failure=';
assert.ok(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2].startsWith(prefix)), 'unsupported arguments');
const control = process.argv[2]?.slice(prefix.length);
const runtimeOwners = selected.filter(name => name.endsWith('::BinanceResponseError') || name.includes('::require_binance'));
assert.equal(runtimeOwners.length, 3);
assert.ok(control === undefined || runtimeOwners.includes(control), 'production control must name one of the three executed APIs; diagnostic controls use native tests');
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(value => value.name === 'safe-cex-venue-binance');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual Binance compiler package required');
const target = pkg.targets.find(value => value.kind.includes('lib'));
assert.ok(target, 'actual production library required');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_binance.json'), 'utf8'));
const found = inventory(doc, {
  defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'),
  readSource: file => fs.readFileSync(path.join(workspace, file)),
});
const types = found.types.filter(value => selected.includes(value.name));
assert.deepEqual(types.map(value => value.name).sort(), selected);
assert.ok(types.every(value => value.deserialize === false));
assert.equal(types.filter(value => value.kind === 'function').length, 6);
assert.equal(types.filter(value => value.kind === 'record').length, 1);
console.log(`Discovery: ${found.types.length} owners, ${found.publicAliases.length} aliases, ${found.construction.length} construction policies.`);
console.log('Probe scope: three selected response-shape outputs/conversion and four diagnostic classifications only; remaining owners/aliases/locals and feature/wasm coverage are excluded. Run accepted probes serially.');
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
assert.equal(results.filter(result => result.classificationOnly === false && result.category === 'decoder').length, 3, 'three actual output/conversion runtime receipts required');
assert.equal(results.filter(result => result.classificationOnly === true && result.category === 'non-decoder').length, 4, 'four explicit diagnostic classifications, not API executions');
if (control) {
  const module = 'safe_cex_venue_binance::response_shape::';
  const dependents = { BinanceResponseError: ['BinanceResponseError', 'require_binance_array_response', 'require_binance_pm_array_response'] };
  const expected = (dependents[control.slice(module.length)] ?? [control.slice(module.length)]).map(ident => module + ident);
  const failed = results.filter(result => result.failures.length);
  assert.deepEqual(failed.map(result => result.name).sort(), expected.sort(), 'only the changed owner and its actual shared-body callers may fail');
  assert.ok(failed.every(result => result.failures.every(failure => failure.startsWith('function output mismatch for '))), 'compiled native output mismatches required; refusals or compile errors do not count');
  console.log(`NATIVE_OUTPUT_FAILURE: ${control}`);
} else {
  for (const result of results) assert.equal(result.failures.length, 0,
    `${result.name}: ${result.failures.join('; ')}`);
  console.log('Three production response-shape output probes and four explicit diagnostic classifications passed. The qualified typed conversion is executed; classification receipts do not execute diagnostic utilities. No whole-venue accepted-set zero claim.');
}
