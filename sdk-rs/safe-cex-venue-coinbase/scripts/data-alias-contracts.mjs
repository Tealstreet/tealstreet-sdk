import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = [
  'market_position_reads::ReadItem', 'market_position_reads::ReadItems',
  'market_position_reads::ReadMarket', 'market_position_reads::ReadMarkets',
  'market_position_reads::ReadTickers', 'market_position_reads::ReadPositions',
  'public_market_reads::CatalogMarkets', 'public_market_reads::CatalogSupportedTypes',
  'public_market_reads::PublicCandles', 'order_sync::PrivateOrderMarkets',
  'exchange_lifecycle::LifecycleTimer',
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
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-data-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-data-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(row => row.failures.length === 1 && row.failures.every(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('All eleven data aliases fail their missing native contracts by name.');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    const capability = ['market_position_reads::ReadTickers', 'market_position_reads::ReadPositions',
      'public_market_reads::CatalogMarkets'].some(name => result.name === `safe_cex_venue_coinbase::${name}`);
    assert.equal(Boolean(result.classificationOnly), capability, `${result.name}: actual decoder capability`);
  }
  console.log('Eight native decoder receipts and three explicit data capability classifications pass.');
}
