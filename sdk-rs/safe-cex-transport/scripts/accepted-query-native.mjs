import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

// Reviewed finite production owner set. The caller supplies fresh compiler
// inventory, never replacement owners/APIs/contracts. Mutation controls retain
// signatures/line counts and change only the observed operation's native effect.
const selectedNames = [
  "safe_cex_transport::how_context::TradeTunnelTimeoutError",
  "safe_cex_transport::provider::axios_form_encode",
  "safe_cex_transport::query::deserialize_query_wire",
  "safe_cex_transport::query::query_json_projection",
  "safe_cex_transport::signature_rejection::response_data",
  "safe_cex_transport::signature_rejection::is_binance_style_signature_rejection",
  "safe_cex_transport::tealstreet::is_tealstreet_web_socket_url",
  "safe_cex_transport::transport_resolution::default_proxy_region",
  "safe_cex_transport::transport_resolution::resolve_account_transport"
];
const args = process.argv.slice(2);
assert.ok(args.length === 2 && args.every(arg => /^--(?:inventory|out)=.+/.test(arg)), 'expected --inventory=FILE --out=FILE');
const inventoryPath = args.find(arg => arg.startsWith('--inventory='))?.slice(12);
const outputPath = args.find(arg => arg.startsWith('--out='))?.slice(6);
assert.ok(inventoryPath && outputPath, 'compiler inventory and native receipt output required');
const sdk = fs.realpathSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..'));
const inv = JSON.parse(fs.readFileSync(inventoryPath)).find(pkg => pkg.package === 'safe-cex-transport');
assert.ok(inv, 'actual Transport compiler inventory required');
const selected = new Set(selectedNames);
const types = inv.types.filter(owner => selected.has(owner.name));
const publicAliases = inv.publicAliases.filter(owner => selected.has(owner.name));
assert.deepEqual([...types, ...publicAliases].map(owner => owner.name).sort(), [...selected].sort(), 'every selected actual compiler owner/alias required');
const metadata = spawnSync('cargo', ['metadata', '--locked', '--format-version=1'], { cwd: sdk, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
assert.equal(metadata.status, 0, metadata.stderr);
const outputs = runProbes(sdk, JSON.parse(metadata.stdout), [{ ...inv, types, publicAliases, locals: [], delegatedLocals: [], defaultWitnesses: [] }], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(outputPath, JSON.stringify(outputs, null, 2) + '\n');
assert.equal(outputs.length, selected.size);
for (const row of outputs) assert.deepEqual(row.failures, [], row.name);
console.log(`${selected.size} selected query/diagnostic Transport production native/default receipts passed; no full-package claim.`);
