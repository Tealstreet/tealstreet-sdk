// Selected public aliases discovered by the maintained compiler inventory.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['spot_types::CancelOrderRequests', 'types::CancelOrderRequests', 'types::Cloid', 'types::FundingHistory', 'types::UserOpenOrders'].map(name => `safe_cex_venue_hyperliquid::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, found, aliases, macroOrigins, artifacts } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-hyperliquid', aliasNames: names, jobs: 1,
});
const artifact = artifacts.find(item => item.package_id === pkg.id && item.target.name === target.name);
assert.ok(artifact && Array.isArray(artifact.features));
assert.ok(macroOrigins.includes('serde_json'), 'actual reviewed serde_json origin must authenticate');
fs.writeFileSync(path.join(metadata.target_directory, 'public-alias-context.json'),
  `${JSON.stringify({ packageId: pkg.id, macroOrigins,
    externalRoots: found.externalTypeAliases.map(alias => ({ name: alias.name,
      canonical: alias.canonical, artifact: alias.artifact })),
    claims: aliases.map(alias => ({ name: alias.name, claim: alias.claim })),
  }, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'public-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, features: artifact.features, types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'public-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(result => result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('All five Hyperliquid wire aliases fail missing classification by name.');
} else {
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
  console.log('All five Hyperliquid wire aliases pass native compiler trait probes.');
}
