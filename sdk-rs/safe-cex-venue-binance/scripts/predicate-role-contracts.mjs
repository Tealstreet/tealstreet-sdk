// Three individually reviewed non-decoder Boolean predicates only. This is not the whole-venue accepted gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env },
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const selected = [
  'safe_cex_venue_binance::order_logic::detect_balance_issue',
  'safe_cex_venue_binance::order_logic::is_binance_position_mode_already_set_error',
  'safe_cex_venue_binance::order_logic::is_post_only_rejection_error',
];
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(value => value.name === 'safe-cex-venue-binance');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual Binance compiler package required');
const target = pkg.targets.find(value => value.kind.includes('lib'));
assert.ok(target, 'actual production library required');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_binance.json'), 'utf8'));
const found = inventory(doc, {
  defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'),
  readSource: file => fs.readFileSync(path.join(workspace, file)),
});
const types = found.types.filter(value => selected.includes(value.name));
assert.deepEqual(types.map(value => value.name).sort(), selected);
assert.ok(types.every(value => value.deserialize === false && value.kind === 'function'));
console.log(`Discovery: ${found.types.length} owners, ${found.publicAliases.length} aliases, ${found.construction.length} construction policies.`);
console.log('Probe scope: three selected predicate classifications only; remaining owners/aliases/locals and feature/wasm coverage are excluded. Run accepted probes serially.');
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
for (const result of results) assert.equal(result.failures.length, 0,
  `${result.name}: ${result.failures.join('; ')}`);
console.log('Three production non-decoder classification receipts passed. Actual predicate behavior is witnessed separately by accepted_predicate_roles and the live order_logic/rejection targets. No whole-venue accepted-set zero claim.');
