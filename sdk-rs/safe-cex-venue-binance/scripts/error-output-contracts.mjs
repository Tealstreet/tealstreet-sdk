// Three individually reviewed decoder outputs, including the private shared helper. This is not the whole-venue accepted gate.
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
const selected = [
  'safe_cex_venue_binance::response_message::get_binance_pm_response_message',
  'safe_cex_venue_binance::response_message::get_binance_response_message',
  'safe_cex_venue_binance::response_message::response_message',
];
const prefix = '--expect-output-failure=';
assert.ok(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2].startsWith(prefix)), 'unsupported arguments');
const control = process.argv[2]?.slice(prefix.length);
assert.ok(control === undefined || selected.includes(control), 'control owner must be selected');
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
assert.ok(types.every(value => value.deserialize === false && value.kind === 'function'));
console.log(`Discovery: ${found.types.length} owners, ${found.publicAliases.length} aliases, ${found.construction.length} construction policies.`);
console.log('Probe scope: three selected decoder outputs only; remaining owners/aliases/locals and feature/wasm coverage are excluded. Run accepted probes serially.');
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
assert.ok(results.every(result => result.classificationOnly === false && result.category === 'decoder'), 'actual decoder-output runtime receipts required');
if (control) {
  const expected = control === 'safe_cex_venue_binance::response_message::response_message'
    ? selected.filter(name => name.includes('::response_message::')) : [control];
  const failed = results.filter(result => result.failures.length);
  assert.deepEqual(failed.map(result => result.name).sort(), expected.sort(), 'only the changed owner and its actual shared-body callers may fail');
  assert.ok(failed.every(result => result.failures.every(failure => failure.startsWith('function output mismatch for '))), 'compiled native output mismatches required; refusals or compile errors do not count');
  console.log(`NATIVE_OUTPUT_FAILURE: ${control}`);
} else {
  for (const result of results) assert.equal(result.failures.length, 0,
    `${result.name}: ${result.failures.join('; ')}`);
  console.log('Three production decoder-output probes passed, including the direct private helper call. No whole-venue accepted-set zero claim.');
}
