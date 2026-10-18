// Reviewed exchange-helper batch only; this is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'exchange-helper batch takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
// Keep proc-macro artifact configuration consistent through discovery and probes.
process.env.RUSTC_BOOTSTRAP = '1';
const selected = [
  'after_method_called_in_worker',
  'assert_algo_order_capacity',
  'assert_v3_feature',
  'fetch_open_interest_map',
  'get_open_algo_orders_count',
  'is_raw_algo_order',
  'map_public_trade',
  'parse_leverage_brackets_response',
  'retained_info',
  'transform_market',
  'transform_position',
  'transform_ticker',
].map(name => `safe_cex_venue_asterdex::exchange_helpers::${name}`).sort();
assert.equal(selected.length, 12);
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
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-exchange-helper-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: actual native execution required`);
}
assert.equal(selected.reduce((count, name) => count + contracts[name].cases.length, 0), 48);
console.log('Aster exchange-helper batch: 12 actual production-native owners / 48 cases, zero failures.');
console.log('Scope is exactly these twelve native function owners; other full-matrix obligations remain.');

const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-exchange-helper-source.cjs', import.meta.url))], {
  cwd: workspace, encoding: 'utf8', env: process.env,
});
assert.equal(source.status, 0, `maintained-source expectation check failed: ${source.stderr}`);
process.stdout.write(source.stdout);
