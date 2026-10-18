import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['LeverageFuture', 'LeverageLoader']
  .map(name => `safe_cex_venue_binance::account_leverage_overlay::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-binance', aliasNames: names, jobs: 1,
});
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'account-leverage-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  for (const result of results) assert.ok(result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract')), result.name);
  console.log('Both account leverage aliases produce named missing-contract failures.');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    assert.equal(result.classificationOnly, true, result.name);
    assert.equal(result.category, 'non-decoder-alias', result.name);
    assert.equal(result.deserialize, false, result.name);
  }
  console.log('Both actual account leverage aliases pass native trait probes.');
}
