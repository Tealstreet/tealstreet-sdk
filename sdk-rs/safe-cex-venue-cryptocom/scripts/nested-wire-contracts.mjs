// Execute the nine reviewed native/default Crypto.com nested record and helper owners, not full discovery.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'selected census runner takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  "safe_cex_venue_cryptocom::wire::CryptocomAccountsResponse",
  "safe_cex_venue_cryptocom::wire::CryptocomBalanceSummary",
  "safe_cex_venue_cryptocom::wire::CryptocomBook",
  "safe_cex_venue_cryptocom::wire::CryptocomBookData",
  "safe_cex_venue_cryptocom::wire::CryptocomOrder",
  "safe_cex_venue_cryptocom::wire::CryptocomTrade",
  "safe_cex_venue_cryptocom::wire::CryptocomTradeFill",
  "safe_cex_venue_cryptocom::wire::deserialize_present",
  "safe_cex_venue_cryptocom::wire::deserialize_present_string"
];
assert.equal(selected.length, 9);
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
const selectedConstruction = selected.filter(name => !name.includes('::deserialize_present'));
assert.equal(selectedConstruction.length, 7);
assert.deepEqual(discovery.construction.map(owner => owner.name).sort(), selectedConstruction);
const construction = loadPolicy('construction');
for (const owner of discovery.construction) {
  assert.ok(construction[owner.name]?.reason?.trim(), `missing specific construction review: ${owner.name}`);
  assert.deepEqual(owner.traits, construction[owner.name].traits, `construction traits changed: ${owner.name}`);
}

const contracts = loadContracts();
for (const name of selected) assert.ok(contracts[name], `missing reviewed selected contract: ${name}`);
const pkg = { ...discovery, package: packageMetadata.name, packageId: packageMetadata.id,
  target, features: [], publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [] };
const configuration = { target: 'native', features: [], jobs: 1 };
const results = runProbes(workspace, metadata, [pkg], contracts, configuration);
fs.writeFileSync(path.join(metadata.target_directory, 'cryptocom-nested-wire-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) assert.deepEqual(row.failures, [], row.name);
assert.equal(results.filter(row => !row.classificationOnly).length, 9);
assert.equal(results.filter(row => row.classificationOnly).length, 0);
console.log('Crypto.com selected nested wire: 9/9 native decoder results, 0 classifications, 0 failed owners.');
console.log('Selected construction: 7 exact compiler traits reviewed; record observations execute actual Clone values.');
console.log('Scope: seven nested wire records and two private presence helpers only; other census owners, aliases, construction, locals, features and wasm are separate obligations.');
