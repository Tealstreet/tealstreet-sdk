import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = [
  'api_factory::ApiRequest', 'cdp_signing::CdpCryptoKey',
  'exchange_ws_auth::ExchangeWsAuthAsync', 'exchange_lifecycle::LifecycleAccountRoot',
  'public_market_reads::PublicOHLCVOptionsRoot', 'private_stream_dispatch::PrivateOptions',
  'order_sync::PrivateOrderEventRoot', 'order_edit::EditUpdateRoot',
].map(name => `safe_cex_venue_coinbase::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, aliases, macroOrigins, found } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-coinbase', aliasNames: names, jobs: 1,
});
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  features: [], defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-config-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-config-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(row => row.failures.length === 1 && row.failures.every(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('All eight config aliases fail their missing native contracts by name.');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    assert.equal(result.classificationOnly, true, `${result.name}: non-decoder config capability`);
  }
  console.log('All eight config aliases pass their native compiler classifications.');
}
