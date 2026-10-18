// Reviewed stream batch only; this is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { contractFor } from '../../safe-cex-parity/scripts/accepted-contracts.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.ok(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2] === '--preflight'), 'stream batch takes only --preflight');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  ...['is_partial_depth_stream', 'private_listen_key_expired', 'private_on_open',
    'private_ping_with', 'public_event_data_dispatch', 'public_kline_update',
    'public_mark_price_update', 'public_ping_with', 'public_stream_dispatch',
    'public_stream_trade_update', 'public_subscribe_plan', 'public_ticker_update',
    'public_unsubscribe_plan'].map(name => `ws_helpers::${name}`),
  ...['optional_id', 'parsed', 'status', 'stringify', 'text'].map(name => `private_stream::${name}`),
  'public_stream::string',
].map(name => `safe_cex_venue_asterdex::${name}`).sort();
assert.equal(selected.length, 19);
if (!process.argv.includes('--preflight')) {
  const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-stream-source.cjs', import.meta.url))], {
    cwd: workspace, encoding: 'utf8', env: process.env,
  });
  assert.equal(source.status, 0, `actual TS stream expectations failed: ${source.error ?? ''}\n${source.stderr}`);
  process.stdout.write(source.stdout);
}

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
const classification = found.types.map(owner => {
  try { contractFor(owner, contracts); return { name: owner.name, error: null }; }
  catch (error) { return { name: owner.name, error: error.message }; }
});
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-stream-classifications.json'), JSON.stringify(classification, null, 2) + '\n');
for (const row of classification) if (row.error) console.error(row.error);
assert.deepEqual(classification.filter(row => row.error), [], 'each selected stream decoder needs a reviewed native contract');
if (process.argv.includes('--preflight')) { console.log('Aster stream classifications: 19/19 PASS (runtime not executed).'); process.exit(0); }
for (const name of selected) assert.equal(contracts[name].category, 'decoder', name);
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-stream-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: actual native execution required`);
}
console.log('Aster stream batch: 19 actual production-native owners, zero failures.');
console.log('Scope excludes other owners, aliases, locals, features and wasm; this is not a full package gate.');
