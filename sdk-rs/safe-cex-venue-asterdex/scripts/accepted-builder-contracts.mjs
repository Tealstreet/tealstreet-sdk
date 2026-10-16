// Reviewed builder batch only; this is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'builder batch takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  "safe_cex_venue_asterdex::builder::AsterBuilderApprovalCheckResult",
  "safe_cex_venue_asterdex::builder::AsterBuilderApprovalResult",
  "safe_cex_venue_asterdex::builder::AsterBuilderApprovalStatus",
  "safe_cex_venue_asterdex::builder::AsterBuilderFeeConfig",
  "safe_cex_venue_asterdex::builder::assert_fee_rate",
  "safe_cex_venue_asterdex::builder::extract_builder_rows",
  "safe_cex_venue_asterdex::builder::get_base_url",
  "safe_cex_venue_asterdex::builder::get_provider_error_code",
  "safe_cex_venue_asterdex::builder::get_provider_error_message",
  "safe_cex_venue_asterdex::builder::normalize_decimal_string",
  "safe_cex_venue_asterdex::builder::parse_fee_rate",
  "safe_cex_venue_asterdex::builder::parse_scaled_decimal"
];
assert.equal(selected.length, 12);
const source = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-builder-source.cjs', import.meta.url))], {
  cwd: workspace, encoding: 'utf8', env: process.env,
});
assert.equal(source.status, 0, `actual TS expectations failed: ${source.error ?? ''}\n${source.stderr}`);
assert.match(source.stdout, /8 helpers \/ 55 expectations; 4 erased record schemas \/ 27 shape witnesses pass/);
process.stdout.write(source.stdout);
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extraEnv },
  });
  assert.equal(result.status, 0, `${args.join(' ')}: ${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(item => item.name === 'safe-cex-venue-asterdex');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(item => item.kind.includes('lib'));
assert.ok(target);
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
assert.equal(doc.index[String(doc.root)].name, target.name);
const policyRoot = path.join(workspace, 'safe-cex-parity/accepted-set');
const found = inventory(doc, {
  onlyOwners: selected,
  defaultTraits: loadPolicy('default-traits', policyRoot),
  receiverTraits: loadPolicy('receiver-traits', policyRoot),
  readSource: file => fs.readFileSync(path.resolve(workspace, file)),
});
assert.deepEqual(found.types.map(owner => owner.name).sort(), selected);
assert.equal(found.types.filter(owner => owner.kind === 'function').length, 8);
const records = selected.filter(name => name.includes('::AsterBuilder'));
assert.deepEqual(found.construction, [], 'public builder data has no gated private construction');
const contracts = loadContracts();
for (const name of selected) {
  if (records.includes(name)) assert.ok(contracts[name]?.nativeObservation, name);
  else assert.equal(contracts[name]?.category, 'decoder', name);
}
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-builder-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: actual native execution required`);
}
console.log('Aster builder batch: 12 actual production-native owners, zero failures; four native Clone observations.');
console.log('Scope excludes other owners, aliases, locals, features and wasm; this is not a full package gate.');
