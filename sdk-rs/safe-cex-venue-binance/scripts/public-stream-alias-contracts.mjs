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
  'public_ticker_events::Failure', 'public_ticker_events::TickerFields',
  'public_dispatch::CallbackList', 'public_dispatch::CoreInvoke',
  'orderbook::BinanceDepthUpdate',
].map(name => `safe_cex_venue_binance::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => ['--expect-unclassified', '--controls'].includes(arg)));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-binance', aliasNames: names, jobs: 1,
});
assert.ok(aliases.every(alias => alias.generics.params.length === 0));
assert.ok(macroOrigins.includes('serde_json'), 'actual reviewed serde_json origin must authenticate');
fs.writeFileSync(path.join(metadata.target_directory, 'public-stream-alias-context.json'),
  `${JSON.stringify({ packageId: pkg.id, macroOrigins,
    externalRoots: found.externalTypeAliases.map(alias => ({ name: alias.name,
      canonical: alias.canonical, artifact: alias.artifact })),
    claims: aliases.map(alias => ({ name: alias.name, claim: alias.claim })),
  }, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'public-stream-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
const packages = [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}];
const contracts = loadContracts();
const probe = reviewed => runProbes(root, metadata, packages, reviewed,
  { target: 'native', features: [], jobs: 1 });
const results = probe(contracts);
fs.writeFileSync(path.join(metadata.target_directory, 'public-stream-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(result => result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('All five public stream aliases fail missing classification by name.');
} else {
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
  console.log('All five public stream aliases pass their native compiler contracts.');
}
if (args.includes('--controls')) {
  const name = 'safe_cex_venue_binance::orderbook::BinanceDepthUpdate';
  const original = contracts[name].observation.call;
  for (const [label, mutation] of [
    ['opaque member removal', 'decoded.remove("opaque");'],
    ['signed-zero corruption', 'if let Some(serde_json::Value::Object(opaque))=decoded.get_mut("opaque") { opaque.insert("neg".into(), serde_json::json!(0.0)); }'],
  ]) {
    const faulty = structuredClone(contracts);
    faulty[name].observation.call = original.replace('let decoded:', 'let mut decoded:')
      .replace('witness::depth(&decoded)', `${mutation} witness::depth(&decoded)`);
    assert.notEqual(faulty[name].observation.call, original);
    const failed = probe(faulty);
    const owner = failed.find(result => result.name === name);
    assert.ok(!owner.classificationOnly && owner.failures.some(failure =>
      failure.includes('Deserialize output mismatch')), `${label}: native decoded-value control must fail`);
    assert.ok(failed.filter(result => result.name !== name).every(result => !result.failures.length));
    const restored = probe(contracts);
    assert.ok(restored.every(result => !result.failures.length));
    console.log(`${label}: compiled named native RED; exact contract restoration GREEN`);
  }
}
