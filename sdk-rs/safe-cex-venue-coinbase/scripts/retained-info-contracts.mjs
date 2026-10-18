import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {inventory} from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
process.env.CARGO_TARGET_DIR = path.join(root, 'target/retained-info-contracts');
const names = ['safe_cex_venue_coinbase::portfolios_balances::PortfolioState', 'safe_cex_venue_coinbase::orders::map_coinbase_rest_order', 'safe_cex_venue_coinbase::rest_reads::format_spot_cfm_tickers', 'safe_cex_venue_coinbase::utils::normalize_cfm_contract_size'];
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: {...process.env, CARGO_BUILD_JOBS: '1', ...env}});
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--no-deps', '--format-version=1']));
const pkg = metadata.packages.find(row => row.name === 'safe-cex-venue-coinbase');
const target = pkg.targets.find(row => row.kind.includes('lib'));
cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs', '1', '--', '-Z', 'unstable-options',
  '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], {RUSTC_BOOTSTRAP: '1'});
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
const selected = inventory(doc, {onlyOwners: names});
const contracts = loadContracts();
const results = runProbes(root, metadata, [{...selected, package: pkg.name, packageId: pkg.id, target,
  features: [], locals: [], delegatedLocals: [], publicAliases: [], defaultWitnesses: []}], contracts,
  {target: 'native', features: [], jobs: 1});
assert.deepEqual(results.map(row => row.name).sort(), [...names].sort());
for (const result of results) assert.deepEqual(result.failures, [], result.name);
console.log(`${pkg.name}: ${results.length} retained-info native contracts passed`);
