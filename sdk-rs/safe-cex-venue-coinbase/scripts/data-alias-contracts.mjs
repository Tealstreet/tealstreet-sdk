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
].map(name => `safe_cex_venue_coinbase::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => ['--expect-unclassified', '--controls'].includes(arg)));
const { metadata, pkg, target, aliases, macroOrigins, found } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-coinbase', aliasNames: names, jobs: 1,
});
const packages = [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  features: [], defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}];
const probe = contracts => runProbes(root, metadata, packages, contracts,
  { target: 'native', features: [], jobs: 1 });
const contracts = loadContracts();
const results = probe(contracts);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-data-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-data-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(row => row.failures.length === 1 && row.failures.every(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('All ten data aliases fail their missing native contracts by name.');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    const capability = ['market_position_reads::ReadTickers', 'market_position_reads::ReadPositions',
      'public_market_reads::CatalogMarkets'].some(name => result.name === `safe_cex_venue_coinbase::${name}`);
    assert.equal(Boolean(result.classificationOnly), capability, `${result.name}: actual decoder capability`);
  }
  console.log('Seven native decoder receipts and three explicit data capability classifications pass.');
}

if (args.includes('--controls')) {
  for (const [suffix, before, after] of [
    ['market_position_reads::ReadItems', 'let rows=decoded.borrow();',
      'let mut rows=decoded.borrow_mut(); rows.truncate(1);'],
    ['market_position_reads::ReadMarket', 'market(&decoded.borrow())',
      '{let mut row=decoded.borrow_mut(); row.info=None; market(&row)}'],
  ]) {
    const name = `safe_cex_venue_coinbase::${suffix}`;
    const faulty = structuredClone(contracts);
    const original = faulty[name].observation.call;
    assert.ok(original.includes(before));
    faulty[name].observation.call = original.replace(before, after);
    const failed = probe(faulty);
    const owner = failed.find(row => row.name === name);
    assert.ok(!owner.classificationOnly && owner.failures.some(failure =>
      failure.includes('Deserialize output mismatch')), `${name}: actual native mutation must fail`);
    assert.ok(failed.filter(row => row.name !== name).every(row => !row.failures.length));
    const restored = probe(contracts);
    assert.ok(restored.every(row => !row.failures.length));
    console.log(`${name}: compiled native observation loss RED; exact restoration GREEN`);
  }
}
