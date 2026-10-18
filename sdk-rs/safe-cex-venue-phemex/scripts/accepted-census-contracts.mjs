// Execute the closed original native/default Phemex census, not full discovery.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

assert.equal(process.argv.length, 2, 'selected census runner takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  "safe_cex_venue_phemex::account::get_account",
  "safe_cex_venue_phemex::private_session::units",
  "safe_cex_venue_phemex::public_routing::units",
  "safe_cex_venue_phemex::rest_account_control::endpoint",
  "safe_cex_venue_phemex::rest_account_control::find_side",
  "safe_cex_venue_phemex::rest_account_control::text",
  "safe_cex_venue_phemex::rest_data::choice",
  "safe_cex_venue_phemex::rest_data::currency_matches",
  "safe_cex_venue_phemex::rest_data::endpoint",
  "safe_cex_venue_phemex::rest_data::is",
  "safe_cex_venue_phemex::rest_data::label",
  "safe_cex_venue_phemex::rest_data::number_field",
  "safe_cex_venue_phemex::rest_data::parsed",
  "safe_cex_venue_phemex::rest_data::units",
  "safe_cex_venue_phemex::rest_data::validate_account",
  "safe_cex_venue_phemex::rest_order_control::endpoint",
  "safe_cex_venue_phemex::start_order_formatting::ExchangeState",
  "safe_cex_venue_phemex::start_order_formatting::field_is",
  "safe_cex_venue_phemex::start_order_formatting::get",
  "safe_cex_venue_phemex::start_order_formatting::is",
  "safe_cex_venue_phemex::start_order_formatting::opt",
  "safe_cex_venue_phemex::start_order_formatting::u",
  "safe_cex_venue_phemex::ticker_parser::fetch_tickers",
  "safe_cex_venue_phemex::ticker_parser::numeric",
  "safe_cex_venue_phemex::ticker_parser::parse_tickers",
  "safe_cex_venue_phemex::ticker_parser::type_error",
  "safe_cex_venue_phemex::ticker_reads::error_field",
  "safe_cex_venue_phemex::ticker_reads::fetch_tickers_raw",
  "safe_cex_venue_phemex::types::PhemexAccount",
  "safe_cex_venue_phemex::types::PhemexApiResponse",
  "safe_cex_venue_phemex::types::PhemexPosition",
  "safe_cex_venue_phemex::types::PositionsResponse",
  "safe_cex_venue_phemex::utilities::format_symbol_in",
  "safe_cex_venue_phemex::utilities::format_symbol_out",
  "safe_cex_venue_phemex::validation::balance",
  "safe_cex_venue_phemex::validation::validate_account",
  "safe_cex_venue_phemex::websocket_data::PublicSocket",
  "safe_cex_venue_phemex::websocket_data::get",
  "safe_cex_venue_phemex::websocket_data::is",
  "safe_cex_venue_phemex::websocket_data::name",
  "safe_cex_venue_phemex::websocket_data::opt",
  "safe_cex_venue_phemex::websocket_data::parse_field",
  "safe_cex_venue_phemex::websocket_data::send",
  "safe_cex_venue_phemex::websocket_data::u"
];
assert.equal(selected.length, 44);
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', CARGO_INCREMENTAL: '0', ...extraEnv },
  });
  assert.equal(result.status, 0, `native census compiler command failed: ${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version', '1']));
const packageMetadata = metadata.packages.find(pkg => pkg.name === 'safe-cex-venue-phemex');
assert.ok(packageMetadata);
const target = packageMetadata.targets.find(item => item.kind.includes('lib'));
assert.ok(target);
cargo(['rustdoc', '--locked', '-p', packageMetadata.name, '--jobs', '1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
const discovery = inventory(doc, { onlyOwners: selected, readSource: file => fs.readFileSync(path.resolve(workspace, file)) });
assert.deepEqual(discovery.types.map(type => type.name).sort(), selected);
const contracts = loadContracts();
for (const name of selected) assert.ok(contracts[name], `missing reviewed selected contract: ${name}`);
const pkg = { ...discovery, package: packageMetadata.name, packageId: packageMetadata.id,
  target, features: [], publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [] };
const configuration = { target: 'native', features: [], jobs: 1 };
const results = runProbes(workspace, metadata, [pkg], contracts, configuration);
fs.writeFileSync(path.join(metadata.target_directory, 'phemex-selected-census-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) assert.deepEqual(row.failures, [], row.name);
assert.equal(results.filter(row => !row.classificationOnly).length, 30);
assert.equal(results.filter(row => row.classificationOnly).length, 14);
console.log('Phemex selected census44:30 native decoder results +14 classifications,0 failed owners.');
console.log('Scope: original native/default census only; post-census owners, aliases, locals, features and wasm are separate obligations.');
