// Eleven selected owners only; this is not a whole-crate/feature/wasm claim.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
import { contractFor } from '../../safe-cex-parity/scripts/accepted-contracts.mjs';

assert.ok(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2] === '--preflight'));
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selectedCounts = {
  'account_fetch::decode': 15,
  'account_fetch::mapper_error': 4,
  'account_helpers::get_account_update_upnl_deltas': 11,
  'account_helpers::handle_account_update': 7,
  'account_helpers::handle_execution_report': 17,
  'account_helpers::mirror_cumulative_order_fill': 12,
  'account_helpers::parse_asset_balance_updates': 10,
  'api_helpers::generate_aster_dex_code_message_signature': 7,
  'api_helpers::generate_v1_signature': 4,
  'types::opaque_fields': 8,
  'types::present': 36,
};
const selected = Object.keys(selectedCounts).map(name => 'safe_cex_venue_asterdex::' + name).sort();
assert.equal(selected.length, 11);
assert.equal(Object.values(selectedCounts).reduce((a, b) => a + b), 131);
process.env.RUSTC_BOOTSTRAP = '1';
const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-account-source.cjs', import.meta.url))], {
  cwd: workspace, env: process.env, encoding: 'utf8',
});
assert.equal(source.status, 0, 'actual source expectations failed: ' + (source.error ?? '') + '\n' + source.stderr);
assert.match(source.stdout, /7 owners \/ 68 expectations pass/);
process.stdout.write(source.stdout);
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extraEnv },
  });
  assert.equal(result.status, 0, args.join(' ') + ': ' + (result.error ?? '') + '\n' + result.stderr);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(item => item.name === 'safe-cex-venue-asterdex');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(item => item.kind.includes('lib'));
assert.ok(target);
// Never invalidate the ordinary native cache with bootstrap rustdoc artifacts.
const discovery = path.join(metadata.target_directory, 'accepted-inventory');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--', '-Z', 'unstable-options',
  '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'],
{ CARGO_TARGET_DIR: discovery });
const doc = JSON.parse(fs.readFileSync(path.join(discovery, 'doc', target.name + '.json')));
assert.equal(doc.index[String(doc.root)].name, target.name);
const policyRoot = path.join(workspace, 'safe-cex-parity/accepted-set');
const found = inventory(doc, {
  onlyOwners: selected,
  defaultTraits: loadPolicy('default-traits', policyRoot),
  receiverTraits: loadPolicy('receiver-traits', policyRoot),
  readSource: file => fs.readFileSync(path.resolve(workspace, file)),
});
assert.deepEqual(found.types.map(owner => owner.name).sort(), selected);
assert.ok(found.types.every(owner => owner.kind === 'function'));
const contracts = loadContracts();
const zeroCase = contracts['safe_cex_venue_asterdex::types::present'].cases
  .find(row => row.output.value?.bits === '8000000000000000');
assert.ok(zeroCase && Object.is(zeroCase.input.value, -0), 'native corpus must retain its negative-zero input');
for (const owner of found.types) {
  const contract = contractFor(owner, contracts);
  assert.equal(contract.category, 'decoder', owner.name);
  assert.equal(contract.cases.length, selectedCounts[owner.name.slice('safe_cex_venue_asterdex::'.length)], owner.name);
}
if (process.argv[2] === '--preflight') {
  console.log('Aster account preflight: 11 exact compiled owners classified; native probes not executed.');
  process.exit(0);
}
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-account-function-results.json'), JSON.stringify(results, null, 2) + '\n');
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, row.name + ': production native execution required');
}
console.log('Aster account cohort: 11 production-native owners / 131 observations, zero failures or classification-only receipts.');
