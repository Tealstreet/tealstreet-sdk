import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const names = [
  "safe_cex_venue_backpack::adapter::DateParser",
  "safe_cex_venue_backpack::api_factory::FailureHandler",
  "safe_cex_venue_backpack::api_factory::Options",
  "safe_cex_venue_backpack::api_factory::RequestHandler",
  "safe_cex_venue_backpack::api_factory::ResponseHandler",
  "safe_cex_venue_backpack::api_factory::Task",
  "safe_cex_venue_backpack::native_http::BuildHttp",
  "safe_cex_venue_backpack::native_http::InstallTransport",
  "safe_cex_venue_backpack::order_actions::Options",
  "safe_cex_venue_backpack::order_actions::Task",
  "safe_cex_venue_backpack::private_events::TimeoutCallback",
  "safe_cex_venue_backpack::public_listeners::Action",
  "safe_cex_venue_backpack::public_listeners::BookCallback",
  "safe_cex_venue_backpack::public_listeners::CandleCallback",
  "safe_cex_venue_backpack::public_listeners::Fetcher",
  "safe_cex_venue_backpack::public_listeners::Task",
  "safe_cex_venue_backpack::rest_reads::Reply",
  "safe_cex_venue_backpack::rest_reads::Task",
  "safe_cex_venue_backpack::socket_events::TickerCallback",
  "safe_cex_venue_backpack::startup_recovery::Callback"
];
const root = fileURLToPath(new URL('../..', import.meta.url));
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg =>
  ['--expect-unclassified', '--controls'].includes(arg)));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-backpack', aliasNames: names, jobs: 1,
});
const packages = [{
  package: pkg.name, packageId: pkg.id, features: [], target, types: [],
  publicAliases: aliases, defaultWitnesses: [], locals: [], delegatedLocals: [],
  macroOrigins, externalTypeAliases: found.externalTypeAliases,
}];
const contracts = loadContracts();
const run = selected => runProbes(root, metadata, packages, selected,
  { target: 'native', features: [], jobs: 1 });
const requireUnclassified = results => {
  assert.deepEqual(results.map(row => row.name).sort(), names);
  for (const result of results) assert.ok(result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract')), result.name);
  console.log('All 20 Backpack callable/nominal aliases fail missing classification by name.');
};
if (args.includes('--controls')) {
  const missing = { ...contracts };
  for (const name of names) delete missing[name];
  requireUnclassified(run(missing));
}
const results = run(contracts);
assert.deepEqual(results.map(row => row.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  requireUnclassified(results);
} else {
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
  console.log('Backpack20 actual compiler/callable alias receipts PASS');
}
