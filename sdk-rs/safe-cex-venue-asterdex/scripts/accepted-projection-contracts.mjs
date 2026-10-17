// Reviewed projection batch only; this is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'projection batch takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
// Keep proc-macro artifact configuration consistent through discovery and probes.
process.env.RUSTC_BOOTSTRAP = '1';
const selected = [
  'api_helpers::build_v3_nonce', 'api_helpers::build_aster_dex_v3_nonce',
  'builder::build_aster_approve_builder_typed_data', 'builder::is_aster_maker_only_order',
  'builder::get_aster_builder_fee_rate_for_order', 'builder::get_aster_builder_approval_config',
  'builder::resolve_aster_builder_fee_config',
  'exchange_helpers::parse_stp_mode_response', 'exchange_helpers::build_change_stp_mode_request',
  'exchange_helpers::build_change_margin_type_request', 'exchange_helpers::get_fapi_path',
  'exchange_helpers::get_account_from_alias', 'exchange_helpers::transform_order_book',
  'exchange_helpers::balance_from_account_response', 'exchange_helpers::reverse_map_order_status',
  'exchange_helpers::reverse_map_order_type', 'exchange_helpers::extract_filter_value',
  'exchange_helpers::collect_open_interest_from_payload', 'account_helpers::parse_order_number',
  'ws_helpers::public_connection_url',
].map(name => `safe_cex_venue_asterdex::${name}`).sort();
assert.equal(selected.length, 20);
const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-projection-source.cjs', import.meta.url))], {
  cwd: workspace, encoding: 'utf8', env: process.env,
});
assert.equal(source.status, 0, `actual TS expectations failed: ${source.error ?? ''}\n${source.stderr}`);
assert.match(source.stdout, /20 owners \/ 274 expectations pass/);
process.stdout.write(source.stdout);
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extraEnv },
  });
  assert.equal(result.status, 0, `${args.join(' ')}: ${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(item => item.name === 'safe-cex-venue-asterdex');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(item => item.kind.includes('lib'));
assert.ok(target);
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
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
for (const name of selected) assert.equal(contracts[name]?.category, 'decoder', name);
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-projection-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: actual native execution required`);
}
console.log('Aster projection batch: 20 actual production-native owners, zero failures.');
console.log('Scope excludes other owners, aliases, locals, features and wasm; this is not a full package gate.');
