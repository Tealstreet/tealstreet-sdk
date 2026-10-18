// Finite native alias measurement over fresh compiler/artifact/source claims.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['safe_cex_venue_kraken::public_subscriptions::Topic'];
const helpers = ['topic', 'interval_json', 'spot_frame', 'frame_json'].map(name => `safe_cex_venue_kraken::binding::sockets::${name}`);
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => ['--expect-unclassified', '--expect-helper-unclassified'].includes(arg)));
const { metadata, pkg, target, found, aliases, macroOrigins, artifacts } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-kraken', aliasNames: names, jobs: 1,
});
const actual = artifacts.filter(row => row.package_id === pkg.id && row.target.name === target.name);
assert.equal(actual.length, 1, 'one actual selected compiler artifact');
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, features: actual[0].features,
  types: found.types.filter(row => helpers.includes(row.name)), publicAliases: aliases, defaultWitnesses: [], locals: [],
  delegatedLocals: [], macroOrigins, externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'kraken-topic-alias-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), [...names, ...helpers].sort());
if (args.includes('--expect-unclassified')) {
  assert.ok(results.filter(row => names.includes(row.name)).every(row => row.failures.some(failure => failure.includes('public alias needs an accepted-set contract'))));
  console.log('Topic alias: actual native alias missing-classification RED');
} else if (args.includes('--expect-helper-unclassified')) {
  assert.ok(results.filter(row => names.includes(row.name)).every(row => row.failures.length === 0));
  assert.ok(results.filter(row => helpers.includes(row.name)).every(row => row.failures.some(failure => failure.includes('unclassified deserializable type'))));
  console.log('Outgoing helper roles: four actual missing-classification RED receipts');
} else {
  assert.ok(results.every(row => row.failures.length === 0), JSON.stringify(results));
  console.log('Topic native identity/Serde/conversion probes and four outgoing-helper role classifications GREEN; execution proved separately by physical binding witnesses');
}
