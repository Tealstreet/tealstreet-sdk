// Selected public aliases discovered by the maintained compiler inventory.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = [
  'safe_cex_venue_lighter::transaction_signing::SigningResult',
  'safe_cex_venue_lighter::transaction_signing::CryptoCallResult',
  'safe_cex_venue_lighter::transaction_promise::Shared',
  'safe_cex_venue_lighter::transaction_promise::ObjectPromise',
  'safe_cex_venue_lighter::transaction_promise::ObjectResolver',
  'safe_cex_venue_lighter::transaction_promise::PromiseResult',
  'safe_cex_venue_lighter::http_models::Utf16Record',
  'safe_cex_venue_lighter::sdk_validation::ValidationResult',
  'safe_cex_venue_lighter::ws_private_warnings::ReadonlyWarningTimes',
];
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-lighter', aliasNames: names, jobs: 1,
});
assert.ok(macroOrigins.includes('serde_json'), 'actual reviewed serde_json origin must authenticate');
fs.writeFileSync(path.join(metadata.target_directory, 'lighter-structural-alias-context.json'),
  `${JSON.stringify({ packageId: pkg.id, macroOrigins,
    externalRoots: found.externalTypeAliases.map(alias => ({ name: alias.name,
      canonical: alias.canonical, artifact: alias.artifact })),
    claims: aliases.map(alias => ({ name: alias.name, claim: alias.claim })),
  }, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'lighter-structural-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, features: [], types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'lighter-structural-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names.slice().sort());
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(result => result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('The actual Lighter structural aliases fails missing classification by name.');
} else {
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
  console.log('The actual Lighter structural aliases pass native compiler trait probes.');
}
