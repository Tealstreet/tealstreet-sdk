// Reviewed decoder batch only; this is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'decoder batch takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
process.env.RUSTC_BOOTSTRAP = '1';
const selected = [
  ...['AsterDEXMarketResponseFiltersItem', 'AsterDEXMarketResponse',
    'AsterDEXPositionResponse', 'AsterDEXOrderResponse', 'AsterDEXTickerResponse',
    'AsterDEXKlineResponse', 'AsterDEXWebSocketMessage', 'AsterDEXPlaceOrderOpts']
    .map(name => `types::${name}`),
  'order_execution::ExecutionOrder', 'builder_approval::CheckParams', 'builder_approval::ApproveParams',
].map(name => `safe_cex_venue_asterdex::${name}`).sort();
assert.equal(selected.length, 11);
const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-decoder-source.cjs', import.meta.url))], {
  cwd: workspace, encoding: 'utf8', env: process.env,
});
assert.equal(source.status, 0, `actual TS expectations failed: ${source.error ?? ''}\n${source.stderr}`);
assert.match(source.stdout, /11 owners \/ 738 independent observations pass/);
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
assert.ok(found.types.every(owner => owner.kind === 'record'));
assert.deepEqual(found.construction, [], 'all selected data fields are public');
const contracts = loadContracts();
for (const name of selected) {
  const contract = contracts[name];
  assert.ok(contract?.nativeObservation, name);
  const ident = name.split('::').at(-1);
  const owner = Object.values(doc.index).find(item => item.crate_id === 0 && item.name === ident && item.inner.struct);
  assert.ok(owner, name);
  const shape = owner.inner.struct.kind;
  const fields = (shape.plain?.fields ?? shape.tuple).map(id => doc.index[String(id)].name);
  const observed = contract.nativeObservation.nativeFields.filter(field => !['branch', 'byte_len'].includes(field));
  assert.deepEqual(observed, fields, `${name}: every actual compiler field must be observed in declaration order`);
}
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-decoder-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: actual native execution required`);
}
console.log('Aster decoder batch: 11 actual production-native owners, zero failures.');
console.log('Scope excludes other owners, aliases, locals, features and wasm; this is not a full package gate.');
