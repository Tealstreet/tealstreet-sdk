import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['Portfolio', 'Portfolios', 'SupportedTypes', 'PortfolioFailure', 'Balance', 'Currencies']
  .map(name => `safe_cex_venue_coinbase::portfolios_balances::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, aliases, macroOrigins, found } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-coinbase', aliasNames: names, jobs: 1,
});
assert.ok(aliases.every(alias => !alias.generics.params.length));
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  features: [], defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-portfolio-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-portfolio-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(row => row.failures.length === 1 && row.failures.every(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('All six portfolio aliases fail their missing native contract by name.');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    const decoder = ['Portfolio', 'Portfolios', 'SupportedTypes'].some(name =>
      result.name === `safe_cex_venue_coinbase::portfolios_balances::${name}`);
    assert.equal(Boolean(result.classificationOnly), !decoder, `${result.name}: actual decoder capability`);
  }
  console.log('Three portfolio decoder receipts and three native capability classifications pass.');
}
