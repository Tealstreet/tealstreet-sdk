import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const cohort = {
  rest_history: ['HistoryFuture', 'HistoryGet', 'PageCall', 'PageResolver', 'HistoryLog',
    'HistoryEmit', 'CandleCallback', 'CandleCleanup', 'CandleListen', 'CandleFetch', 'SeedFuture'],
  api_factory: ['ApiFuture', 'RequestInterceptor', 'ResponseInterceptor', 'RemoteSignCall'],
  lifecycle: ['LifecycleFuture', 'LifecycleCall', 'LifecycleAsyncCall', 'TickResolver', 'TickTimer'],
};
const names = Object.entries(cohort).flatMap(([module, aliases]) =>
  aliases.map(name => `safe_cex_venue_binance::${module}::${name}`)).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-binance', aliasNames: names, jobs: 1,
});
assert.ok(aliases.every(alias => alias.generics.params.length === 0));
fs.writeFileSync(path.join(metadata.target_directory, 'lifecycle-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'lifecycle-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
for (const result of results) {
  if (args.includes('--expect-unclassified')) {
    assert.ok(result.failures.some(failure =>
      failure.includes('public alias needs an accepted-set contract')), result.name);
  } else {
    assert.deepEqual(result.failures, [], result.name);
  }
}
console.log(args.includes('--expect-unclassified')
  ? 'All 20 lifecycle/history/API aliases fail missing contracts by name.'
  : 'All 20 lifecycle/history/API aliases pass actual native compiler probes.');
