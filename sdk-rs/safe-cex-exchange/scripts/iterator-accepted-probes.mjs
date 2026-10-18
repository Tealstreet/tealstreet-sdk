// Selected native/default regression scope, not a replacement for parity:accepted.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';

const workspace = fileURLToPath(new URL('../..', import.meta.url));
const owners = [
  'safe_cex_exchange::js_runtime::builtins::cached_iterator_member',
  'safe_cex_exchange::js_runtime::collections::Collection',
  'safe_cex_exchange::js_runtime::collections::iterator_next_method',
  'safe_cex_exchange::js_runtime::collections::iterator_object',
  'safe_cex_exchange::js_runtime::value::Js',
];
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg.startsWith('--owner=')), 'only --owner=REVIEWED_OWNER is supported');
const selected = args.length ? [args[0].slice(8)] : owners;
assert.ok(selected.every(owner => owners.includes(owner)), 'owner must belong to the five reviewed iterator owners');
const contracts = loadContracts();
const engine = spawnSync('node', [fileURLToPath(new URL('../tests/iterator_contract_oracle.cjs', import.meta.url))],
  { encoding: 'utf8', timeout: 30_000 });
assert.equal(engine.status, 0, `independent engine expectations must execute: ${engine.stderr}`);
const expected = JSON.parse(engine.stdout);
assert.deepEqual(contracts[owners[1]].cases, expected.collection, 'Collection cases must match independently authored engine values');
for (const name of ['get_property', 'iterator', 'iterable_values']) {
  const entry = contracts[owners[4]].entrypoints.find(entry => entry.api.name === name);
  assert.deepEqual(entry?.cases, expected.js[name], `Js::${name} cases must match independent engine operations`);
}
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, { cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    timeout: 900_000, env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extraEnv } });
  assert.equal(result.status, 0, `iterator accepted probes: Cargo must compile before behavioral evidence\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(pkg => pkg.name === 'safe-cex-exchange');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual exchange workspace package required');
cargo(['rustdoc', '--locked', '--jobs=1', '-p', pkg.name, '--lib', '--', '-Z', 'unstable-options',
  '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_exchange.json')));
const found = inventory(doc, { onlyOwners: selected });
const receipts = runProbes(workspace, metadata, [{ ...found, package: pkg.name }], contracts, { target: 'native', features: [], jobs: 1 });
assert.equal(receipts.length, selected.length, 'each selected compiler owner must execute or classify');
const failures = receipts.filter(receipt => receipt.failures.length);
for (const receipt of failures) console.error(JSON.stringify(receipt));
assert.equal(failures.length, 0, 'iterator accepted native witnesses must pass');
console.log(`${receipts.length} reviewed iterator owner receipts pass (native/default selected scope).`);
