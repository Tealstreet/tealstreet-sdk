import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { acceptedAliasContext } from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = [
  'cdp_signing::CdpAsync', 'public_market_reads::PublicReadAsync',
  'exchange_lifecycle::LifecycleAsync', 'exchange_tick::TickAsync',
  'order_sync::OrderSyncAsync', 'order_edit::EditAsync',
  'order_sync::SyncedOrderRoot', 'order_sync::SyncedOrders',
  'starbase_auth::CoinbaseStarbaseSessionFailure', 'starbase_auth::CoinbaseStarbaseAuthPromise',
  'starbase_ws_controller::CoinbaseStarbaseActivationPromise',
  'classes::ClassFindMethod', 'classes::ClassSupportedMethod',
].map(name => `safe_cex_venue_coinbase::${name}`).sort();
const args = process.argv.slice(2);
const missing = args.find(arg => arg.startsWith('--expect-unregistered='))?.split('=')[1];
assert.ok(args.length <= 1 && args.every(arg => arg === '--controls' ||
  (arg.startsWith('--expect-unregistered=') && names.includes(missing))));
const { metadata, pkg, target, aliases, macroOrigins, found } = acceptedAliasContext({
  workspace: root, packageName: 'safe-cex-venue-coinbase', aliasNames: names, jobs: 1,
});
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  features: [], defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins,
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-generic-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
fs.writeFileSync(path.join(metadata.target_directory, 'coinbase-generic-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), names);
for (const result of results) {
  if (result.name === missing) {
    assert.equal(result.failures.length, 1);
    assert.match(result.failures[0], /generic alias requires explicit compiler monomorphization coverage/);
  } else {
    assert.deepEqual(result.failures, [], result.name);
    assert.equal(result.classificationOnly, true, `${result.name}: specific role classification`);
  }
}
console.log(missing ? `${missing}: missing registration refused without identity fallback` :
  'All thirteen fixed generic aliases pass their native compiler classifications.');
if (args.includes('--controls')) {
  const policy = path.join(root, 'safe-cex-parity/accepted-set/generic-aliases/safe-cex-venue-coinbase.json');
  const original = fs.readFileSync(policy);
  const child = childArgs => {
    const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), ...childArgs], {
      cwd: root, env: process.env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    });
    assert.equal(result.status, 0, `${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
    process.stdout.write(result.stdout);
  };
  for (const owner of ['cdp_signing::CdpAsync', 'classes::ClassFindMethod']) {
    const name = `safe_cex_venue_coinbase::${owner}`;
    try {
      const faulty = JSON.parse(original);
      assert.ok(Object.hasOwn(faulty, name));
      delete faulty[name];
      fs.writeFileSync(policy, `${JSON.stringify(faulty, null, 2)}\n`);
      child([`--expect-unregistered=${name}`]);
    } finally {
      fs.writeFileSync(policy, original);
    }
    child([]);
    console.log(`${name}: registration refusal RED; exact restoration GREEN`);
  }
}
