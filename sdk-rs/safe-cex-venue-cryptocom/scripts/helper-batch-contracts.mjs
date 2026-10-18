// Execute the seventeen reviewed native/default Crypto.com helper owners, not full discovery.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

assert.equal(process.argv.length, 2, 'selected census runner takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  "safe_cex_venue_cryptocom::api_error::append",
  "safe_cex_venue_cryptocom::api_error::http_message",
  "safe_cex_venue_cryptocom::api_error::optional_get",
  "safe_cex_venue_cryptocom::order_mappings::equals",
  "safe_cex_venue_cryptocom::order_mappings::map_order_type",
  "safe_cex_venue_cryptocom::order_mappings::map_stop_trigger_type",
  "safe_cex_venue_cryptocom::order_mappings::map_time_in_force",
  "safe_cex_venue_cryptocom::private_order_mappings::equals",
  "safe_cex_venue_cryptocom::socket_handlers::caught",
  "safe_cex_venue_cryptocom::socket_handlers::concat",
  "safe_cex_venue_cryptocom::socket_handlers::get",
  "safe_cex_venue_cryptocom::socket_handlers::global",
  "safe_cex_venue_cryptocom::socket_handlers::math",
  "safe_cex_venue_cryptocom::socket_handlers::method",
  "safe_cex_venue_cryptocom::socket_handlers::own",
  "safe_cex_venue_cryptocom::socket_handlers::parsed_field",
  "safe_cex_venue_cryptocom::socket_handlers::parsed_or"
];
assert.equal(selected.length, 17);
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', CARGO_INCREMENTAL: '0', ...extraEnv },
  });
  assert.equal(result.status, 0, `native census compiler command failed: ${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version', '1']));
const packageMetadata = metadata.packages.find(pkg => pkg.name === 'safe-cex-venue-cryptocom');
assert.ok(packageMetadata);
const target = packageMetadata.targets.find(item => item.kind.includes('lib'));
assert.ok(target);
cargo(['rustdoc', '--locked', '-p', packageMetadata.name, '--jobs', '1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
const discovery = inventory(doc, { onlyOwners: selected, readSource: file => fs.readFileSync(path.resolve(workspace, file)) });
assert.deepEqual(discovery.types.map(type => type.name).sort(), selected);
assert.deepEqual(discovery.construction, []);

const contracts = loadContracts();
for (const name of selected) assert.ok(contracts[name], `missing reviewed selected contract: ${name}`);
const pkg = { ...discovery, package: packageMetadata.name, packageId: packageMetadata.id,
  target, features: [], publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [] };
const configuration = { target: 'native', features: [], jobs: 1 };
const results = runProbes(workspace, metadata, [pkg], contracts, configuration);
fs.writeFileSync(path.join(metadata.target_directory, 'cryptocom-helper-batch-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) assert.deepEqual(row.failures, [], row.name);
assert.equal(results.filter(row => !row.classificationOnly).length, 3);
assert.equal(results.filter(row => row.classificationOnly).length, 14);
console.log('Crypto.com selected helper owners: 3 native runtime decoder probes, 14 explicit classifications, 0 failed owners.');
console.log('Direct native witnesses and compiling body controls are separate required evidence for all seventeen owners; classification receipts are not runtime evidence.');
console.log('Scope: these seventeen original native/default helper owners only; other owners, aliases, construction, locals, features and wasm remain owed.');
