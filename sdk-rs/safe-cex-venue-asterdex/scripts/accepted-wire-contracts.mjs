// Fixed companion cohort; all other venue owners retain their own gates.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'Wire cohort takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  "safe_cex_venue_asterdex::builder::AsterBuilderApprovalCheckResultWire",
  "safe_cex_venue_asterdex::builder::AsterBuilderApprovalResultWire",
  "safe_cex_venue_asterdex::builder::AsterBuilderApprovalStatusWire",
  "safe_cex_venue_asterdex::builder::AsterBuilderFeeConfigWire",
  "safe_cex_venue_asterdex::types::AsterDEXAPIVersionWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountResponseAssetsItemWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountResponsePositionsItemWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountUpdateABItemWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountUpdateAPItemWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountUpdateAWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountUpdateEWire",
  "safe_cex_venue_asterdex::types::AsterDEXAccountUpdateWire",
  "safe_cex_venue_asterdex::types::AsterDEXDepthResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXFapiVersionWire",
  "safe_cex_venue_asterdex::types::AsterDEXKlineResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXListenKeyResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXMarketResponseFiltersItemWire",
  "safe_cex_venue_asterdex::types::AsterDEXMarketResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXOpenInterestResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXOrderResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXOrderUpdateEWire",
  "safe_cex_venue_asterdex::types::AsterDEXOrderUpdateOWire",
  "safe_cex_venue_asterdex::types::AsterDEXOrderUpdateWire",
  "safe_cex_venue_asterdex::types::AsterDEXPlaceOrderOptsWire",
  "safe_cex_venue_asterdex::types::AsterDEXPositionModeResponseDualSidePositionStringWire",
  "safe_cex_venue_asterdex::types::AsterDEXPositionModeResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXPositionResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXStpModeWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamDepthEWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamDepthWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamMarkPriceEWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamMarkPriceWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamTickerEWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamTickerWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamTradeEWire",
  "safe_cex_venue_asterdex::types::AsterDEXStreamTradeWire",
  "safe_cex_venue_asterdex::types::AsterDEXTickerResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXTradeResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXUserTradeResponseWire",
  "safe_cex_venue_asterdex::types::AsterDEXV3AuthContextWire",
  "safe_cex_venue_asterdex::types::AsterDEXWebSocketMessageWire"
];
assert.equal(selected.length, 42);
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, { cwd: workspace, encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env } });
  assert.equal(result.status, 0, `${args.join(' ')}: ${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(item => item.name === 'safe-cex-venue-asterdex');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(item => item.kind.includes('lib'));
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
const policyRoot = path.join(workspace, 'safe-cex-parity/accepted-set');
const found = inventory(doc, { onlyOwners: selected,
  defaultTraits: loadPolicy('default-traits', policyRoot),
  receiverTraits: loadPolicy('receiver-traits', policyRoot),
  readSource: file => fs.readFileSync(path.resolve(workspace, file)),
});
assert.deepEqual(found.types.map(owner => owner.name).sort(), selected);
assert.deepEqual(found.construction, [], 'private Wire derives no Clone/Copy capability');
const contracts = loadContracts();
let observations = 0;
for (const owner of found.types) {
  assert.equal(owner.deserialize, true, `${owner.name}: actual Deserialize owner`);
  const contract = contracts[owner.name];
  assert.equal(contract?.shape, 'union', `${owner.name}: internal Serde shape is explicit`);
  const item = Object.values(doc.index).find(item => item.crate_id === 0 && item.name === owner.ident && (item.inner.struct || item.inner.enum));
  assert.ok(item, owner.name);
  const structure = item.inner.struct;
  const fields = structure?.kind?.plain?.fields?.map(id => doc.index[String(id)].name)
    ?? structure?.kind?.tuple?.map((_, i) => `slot_${i}`)
    ?? ['variant', 'ordinal'];
  assert.deepEqual(contract.nativeObservation.nativeFields, [...fields, 'public_accepts'],
    `${owner.name}: every compiler field and public boundary is observed`);
  assert.ok(contract.nativeObservation.cases.filter(row => row.output.ok).length >= 2);
  observations += contract.nativeObservation.cases.length;
}
assert.equal(observations, 607, 'retain every fixed native observation');
const results = runProbes(workspace, metadata, [{ ...found,
  package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-wire-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: native execution is required`);
}
console.log(`Aster Wire cohort: 42 production decoders / ${observations} native observations pass.`);
