// Finite native alias measurement over fresh compiler/artifact/source claims.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['Factory', 'RateLimit'].map(name => `safe_cex_venue_kraken::construction::${name}`);
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, found, aliases, macroOrigins, artifacts } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-kraken', aliasNames: names, jobs: 1,
});
const actual = artifacts.filter(row => row.package_id === pkg.id && row.target.name === target.name);
assert.equal(actual.length, 1, 'one actual selected compiler artifact');
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, features: actual[0].features,
  types: [], publicAliases: aliases, defaultWitnesses: [], locals: [],
  delegatedLocals: [], macroOrigins, externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'kraken-construction-alias-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names.slice().sort());
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(row => row.failures.some(failure => failure.includes('public alias needs an accepted-set contract'))));
  console.log('Construction aliases: actual native alias missing-classification RED');
} else {
  assert.ok(results.every(row => row.failures.length === 0), JSON.stringify(results));
  console.log('Construction aliases: actual native invariant identity, Serde and conversion probes GREEN');
}
