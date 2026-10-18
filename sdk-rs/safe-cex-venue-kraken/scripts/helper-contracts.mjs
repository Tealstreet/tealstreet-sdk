// Four individually reviewed module helpers; other Kraken obligations remain open.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env },
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const selected = ['safe_cex_venue_kraken::helpers::create_kraken_api_error', 'safe_cex_venue_kraken::helpers::create_kraken_spot_client_order_id', 'safe_cex_venue_kraken::helpers::parse_trigger_signal', 'safe_cex_venue_kraken::helpers::sort'];
const runtimeOwner = 'safe_cex_venue_kraken::helpers::parse_trigger_signal';
const prefix = '--expect-output-failure=';
assert.ok(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2].startsWith(prefix)), 'unsupported arguments');
const control = process.argv[2]?.slice(prefix.length);
assert.ok(control === undefined || control === runtimeOwner, 'only the actual trigger decoder may use the production output control');
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(value => value.name === 'safe-cex-venue-kraken');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual Kraken compiler package required');
const target = pkg.targets.find(value => value.kind.includes('lib'));
assert.ok(target, 'actual production library required');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_kraken.json'), 'utf8'));
const found = inventory(doc, {
  onlyOwners: selected,
  readSource: file => fs.readFileSync(path.join(workspace, file)),
});
const types = found.types.filter(value => selected.includes(value.name));
assert.deepEqual(types.map(value => value.name).sort(), selected);
assert.ok(types.every(value => value.deserialize === false));
assert.ok(types.every(value => value.kind === 'function'));
console.log('Selected inventory: four actual module helper functions. Whole-crate receiver refusals and other owners/aliases/locals/construction remain open.');
console.log('Probe scope: one native/default trigger output and three role classifications. Classification receipts do not execute the comparator, error constructor or ID generator; their native witnesses run separately. Run accepted probes serially.');
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
assert.equal(results.filter(result => result.classificationOnly === false && result.category === 'decoder').length, 1);
assert.equal(results.filter(result => result.classificationOnly === true && result.category === 'non-decoder').length, 3);
if (control) {
  const failed = results.filter(result => result.failures.length);
  assert.deepEqual(failed.map(result => result.name), [runtimeOwner], 'only the changed trigger decoder may fail');
  assert.ok(failed.every(result => result.failures.every(failure => failure.startsWith('function output mismatch for '))), 'compiled native output mismatch required; compile errors/refusals cannot count');
  console.log(`NATIVE_OUTPUT_FAILURE: ${runtimeOwner}`);
} else {
  for (const result of results) assert.equal(result.failures.length, 0, `${result.name}: ${result.failures.join('; ')}`);
  console.log('One production helper output probe and three explicit non-decoder classifications passed; zero failed owners. No whole-venue accepted-set claim.');
}
