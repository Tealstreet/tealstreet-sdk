import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = [
  'class_jobs::NativeFuture',
  'position_refresh::PositionFetchStarter', 'position_refresh::PositionTimerCallback',
  'request_weight::Observer', 'request_weight::ObserveGetter', 'request_weight::Fulfilled',
  'request_weight::Rejected', 'request_weight::RejectedFuture',
].map(name => `safe_cex_venue_binance::${name}`).sort();
assert.deepEqual(process.argv.slice(2), []);
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-binance', aliasNames: names, jobs: 1,
});
assert.ok(macroOrigins.includes('serde_json'));
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'async-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
for (const result of results) {
  assert.deepEqual(result.failures, [], result.name);
  assert.equal(result.classificationOnly, true, result.name);
  assert.equal(result.category, 'non-decoder-alias', result.name);
  assert.equal(result.deserialize, false, result.name);
  assert.match(result.monomorphization, /one .*Value.*(?:instantiation|probe)/, result.name);
}
console.log('All eight actual async aliases pass native trait and fixed-type identity probes.');
