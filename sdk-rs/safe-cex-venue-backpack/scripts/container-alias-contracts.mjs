import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const names = [
  "safe_cex_venue_backpack::account_reads::ValidationFailure",
  "safe_cex_venue_backpack::api_factory::Headers",
  "safe_cex_venue_backpack::order_actions::Request",
  "safe_cex_venue_backpack::order_utils::BatchRequests",
  "safe_cex_venue_backpack::orderbook_state::DepthLevels",
  "safe_cex_venue_backpack::orderbook_state::StateRef",
  "safe_cex_venue_backpack::orderbook_state::TerminalHandlers",
  "safe_cex_venue_backpack::private_auth::AuthSignature",
  "safe_cex_venue_backpack::public_control::JsString",
  "safe_cex_venue_backpack::public_control::Streams",
  "safe_cex_venue_backpack::public_control::SubscriptionState",
  "safe_cex_venue_backpack::public_listeners::Book",
  "safe_cex_venue_backpack::public_listeners::State",
  "safe_cex_venue_backpack::rest_reads::Array",
  "safe_cex_venue_backpack::signing::SigningRecord",
  "safe_cex_venue_backpack::startup_recovery::NativeMap"
];
const root = fileURLToPath(new URL('../..', import.meta.url));
const { metadata, pkg, target, found, aliases, macroOrigins } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-backpack', aliasNames: names, jobs: 1,
});
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, features: [], target, types: [],
  publicAliases: aliases, defaultWitnesses: [], locals: [], delegatedLocals: [],
  macroOrigins, externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.deepEqual(results.map(row => row.name).sort(), names);
for (const result of results) assert.deepEqual(result.failures, [], result.name);
console.log('Backpack16 actual compiler/container alias receipts PASS');
