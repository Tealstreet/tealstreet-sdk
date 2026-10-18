// Reviewed alternate-owner batch only; other owners and lexical Wire types stay separate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const control = process.argv.slice(2);
assert.ok(control.length === 0 || (control.length === 1 && control[0] === '--missing-contracts-control'), 'alternate gate permits only its fixed missing-contract control');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
// Keep proc-macro artifact configuration consistent through discovery and probes.
process.env.RUSTC_BOOTSTRAP = '1';
const selected = [
  "safe_cex_venue_asterdex::account_fetch::AccountFetch",
  "safe_cex_venue_asterdex::binding::globals::ClockHttp",
  "safe_cex_venue_asterdex::binding::host::Host",
  "safe_cex_venue_asterdex::binding::sockets::Parent",
  "safe_cex_venue_asterdex::builder_approval::ApprovalFailure",
  "safe_cex_venue_asterdex::builder_approval::ApproveParams",
  "safe_cex_venue_asterdex::builder_approval::CheckParams",
  "safe_cex_venue_asterdex::exchange_runtime::AsterDEXExchange",
  "safe_cex_venue_asterdex::listen_key::ListenKeyLifecycle",
  "safe_cex_venue_asterdex::native_exchange::AsterDEXExchange",
  "safe_cex_venue_asterdex::order_execution::ExecutionOrder",
  "safe_cex_venue_asterdex::order_execution::OpenInterestMap",
  "safe_cex_venue_asterdex::order_execution::OrderExecution",
  "safe_cex_venue_asterdex::order_execution::PreparedExecutionOrder",
  "safe_cex_venue_asterdex::private_repairs::PrivateRepairs",
  "safe_cex_venue_asterdex::private_stream::ListenParent",
  "safe_cex_venue_asterdex::private_stream::PrivateStream",
  "safe_cex_venue_asterdex::public_stream::Io",
  "safe_cex_venue_asterdex::public_stream::Protocol",
  "safe_cex_venue_asterdex::public_stream::PublicDataListeners",
  "safe_cex_venue_asterdex::public_stream::PublicStream",
  "safe_cex_venue_asterdex::public_subscriptions::PublicSubscriptions",
  "safe_cex_venue_asterdex::types::AsterDEXKlineResponse",
  "safe_cex_venue_asterdex::types::AsterDEXMarketResponse",
  "safe_cex_venue_asterdex::types::AsterDEXMarketResponseFiltersItem",
  "safe_cex_venue_asterdex::types::AsterDEXOrderResponse",
  "safe_cex_venue_asterdex::types::AsterDEXPlaceOrderOpts",
  "safe_cex_venue_asterdex::types::AsterDEXPositionResponse",
  "safe_cex_venue_asterdex::types::AsterDEXTickerResponse",
  "safe_cex_venue_asterdex::types::AsterDEXWebSocketMessage",
  "safe_cex_venue_asterdex::virtual_clock::AsterDEXClockRealm",
  "safe_cex_venue_asterdex::virtual_clock::AsterDEXVirtualClock",
  "safe_cex_venue_asterdex::virtual_clock::ClockOperation"
];
assert.equal(selected.length, 33);
const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-alternate-source.cjs', import.meta.url))], {
  cwd: workspace, encoding: 'utf8', env: process.env,
});
assert.equal(source.status, 0, `actual TS expectations failed: ${source.error ?? ''}\n${source.stderr}`);
assert.match(source.stdout, /8 owners \/ 435 field and shape expectations pass/);
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
assert.equal(found.types.filter(owner => owner.deserialize).length, 11);
assert.equal(found.types.filter(owner => !owner.deserialize && owner.rawEntrypoints.some(Boolean)).length, 14);
assert.equal(found.types.filter(owner => !owner.deserialize && !owner.rawEntrypoints.some(Boolean)).length, 8);
const contracts = loadContracts();
for (const type of found.types.filter(owner => owner.deserialize)) {
  const item = Object.values(doc.index).find(item => item.crate_id === 0 && item.name === type.ident && item.inner.struct);
  assert.ok(item, type.name);
  const kind = item.inner.struct.kind;
  const ids = kind.plain?.fields ?? kind.tuple;
  const fields = ids.map((id, index) => doc.index[String(id)].name ?? String(index));
  assert.deepEqual(contracts[type.name].nativeObservation.nativeFields, fields,
    `${type.name}: native projection must name every actual compiler field in declaration order`);
}
for (const name of selected) assert.ok(contracts[name], name);
if (control.length) for (const name of selected) delete contracts[name];
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-alternate-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
if (control.length) {
  assert.equal(results.filter(row => row.failures.length).length, 33);
  for (const row of results) assert.ok(row.failures.some(f => f.includes('unclassified deserializable type')), row.name);
  console.log('Aster alternate control: all 33 exact compiler owners compiled and reported missing classification.');
} else {
  for (const row of results) assert.deepEqual(row.failures, [], row.name);
  const typed = results.filter(row => row.classificationOnly).map(row => row.name).sort();
  const expectedTyped = found.types.filter(owner => !owner.deserialize && !owner.rawEntrypoints.some(Boolean)).map(owner => owner.name).sort();
  assert.deepEqual(typed, expectedTyped, 'CI runProbes accepts exactly eight genuinely typed API classifications');
  assert.equal(results.filter(row => !row.classificationOnly).length, 25);
  console.log('Aster alternate batch: 25 production-native decoder/runtime observations and eight explicit typed API receipts; zero failures.');
}
console.log('Scope is the assigned 33 native/default owners; other owners, aliases, lexical locals, features and wasm remain separate.');
