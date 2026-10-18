import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const before = process.argv.includes('--expect-unclassified');
assert.deepEqual(process.argv.slice(2), before ? ['--expect-unclassified'] : []);
const workspace = fileURLToPath(new URL('../..', import.meta.url));
const names = ['safe_cex_venue_bingx::public_stream::HandlerList'];
const context = acceptedAliasContext({
  workspace, packageName: 'safe-cex-venue-bingx', aliasNames: names, jobs: 1,
});
const artifacts = context.artifacts.filter(item =>
  item.package_id === context.pkg.id && item.target.name === context.target.name);
assert.equal(artifacts.length, 1);
const results = runProbes(context.root, context.metadata, [{
  package: context.pkg.name, packageId: context.pkg.id, features: artifacts[0].features,
  target: context.target, types: [], publicAliases: context.aliases, defaultWitnesses: [],
  locals: [], delegatedLocals: [], macroOrigins: context.macroOrigins,
  externalTypeAliases: context.found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(context.metadata.target_directory, 'bingx-handler-list-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
if (before) {
  assert.ok(results.every(result => result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('HandlerList: named compiler alias refusal confirmed');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    assert.equal(result.classificationOnly, true, result.name);
    assert.equal(result.category, 'non-decoder-alias', result.name);
    assert.equal(result.deserialize, false, result.name);
  }
  console.log('BingX HandlerList: 1 specific native callback-registry classification; 0 failed');
}
