import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['ListAccountsResponse', 'IntxInstrumentsResponse']
  .map(name => `safe_cex_venue_coinbase::types::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg =>
  ['--expect-unclassified', '--controls'].includes(arg)));
const { metadata, pkg, target, aliases, macroOrigins, found } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-coinbase', aliasNames: names, jobs: 1,
});
assert.ok(aliases.every(alias => !alias.generics.params.length));
const packages = [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  features: [], defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}];
const probe = contracts => runProbes(root, metadata, packages, contracts,
  { target: 'native', features: [], jobs: 1 });
const contracts = loadContracts();
const results = probe(contracts);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-wire-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-wire-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(row => row.failures.length === 1 && row.failures.every(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('Both exported wire aliases fail their missing native contract by name.');
} else {
  for (const result of results) {
    assert.deepEqual(result.failures, [], result.name);
    assert.ok(!result.classificationOnly, `${result.name}: actual runtime receipt required`);
  }
  console.log('Both exported wire aliases pass their native decoder observations.');
}
if (args.includes('--controls')) {
  for (const name of names) {
    const faulty = structuredClone(contracts);
    const original = faulty[name].observation.call;
    faulty[name].observation.call = original.replace('Ok(decoded) =>', 'Ok(mut decoded) =>')
      .replace('let length=decoded.len();', 'decoded.truncate(1); let length=decoded.len();');
    assert.notEqual(faulty[name].observation.call, original);
    const failed = probe(faulty);
    const owner = failed.find(row => row.name === name);
    assert.ok(!owner.classificationOnly && owner.failures.some(failure =>
      failure.includes('Deserialize output mismatch')), `${name}: native row loss must fail`);
    assert.ok(failed.filter(row => row.name !== name).every(row => !row.failures.length));
    const restored = probe(contracts);
    assert.ok(restored.every(row => !row.failures.length));
    console.log(`${name}: compiled row-loss RED; exact restoration GREEN`);
  }
}
