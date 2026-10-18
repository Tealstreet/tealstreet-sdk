// Execute the eighteen reviewed native/default Crypto.com websocket decoder owners, not full discovery.
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
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsBalanceSummary",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsBalanceUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsBookData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsBookUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsCandlestickData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsCandlestickUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsOrderData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsOrderUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsPositionBalance",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsPositionData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsPositionUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsTickerData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsTickerUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsTradeData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsTradeUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsValuationData",
  "safe_cex_venue_cryptocom::ws_payloads::CryptocomWsValuationUpdate",
  "safe_cex_venue_cryptocom::ws_payloads::field"
];
assert.equal(selected.length, 18);
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
fs.writeFileSync(path.join(metadata.target_directory, 'cryptocom-ws-record-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) assert.deepEqual(row.failures, [], row.name);
assert.equal(results.filter(row => !row.classificationOnly).length, 18);
assert.equal(results.filter(row => row.classificationOnly).length, 0);
console.log('Crypto.com selected websocket owners: 18 native runtime decoder probes, 0 classifications, 0 failed owners.');
console.log('Direct native field/cardinality/shape witnesses and compiling controls are separate required evidence.');
console.log('Scope: these eighteen original native/default websocket owners only; other owners, aliases, construction, locals, features and wasm remain owed.');
