import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const name = 'safe_cex_venue_backpack::signing::SigningRecord';
const root = fileURLToPath(new URL('../..', import.meta.url));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-backpack', aliasNames: [name], jobs: 1,
});
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, features: [], target, types: [],
  publicAliases: aliases, defaultWitnesses: [], locals: [], delegatedLocals: [],
  macroOrigins, externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.deepEqual(results.map(row => row.name), [name]);
for (const result of results) assert.deepEqual(result.failures, [], result.name);
console.log('SigningRecord actual compiler/container/native observations PASS');
