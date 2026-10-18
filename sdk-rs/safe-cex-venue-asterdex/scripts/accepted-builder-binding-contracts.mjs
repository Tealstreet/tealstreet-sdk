import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.equal(process.argv.length, 2, 'builder/binding cohort takes no scope-changing arguments');
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
process.env.RUSTC_BOOTSTRAP = '1';
const selected = [
  ...['build_aster_approval_request', 'get_aster_builder_fee_rate_for_order',
    'is_aster_maker_only_order', 'present'].map(name => `builder::${name}`),
  ...['approve_aster_builder_fee_with_browser_wallet', 'check_aster_builder_approved',
    'check_result', 'form_component', 'no_wallet', 'normalize_address', 'result',
    'switch_to_aster_signature_chain', 'wallet_request'].map(name => `builder_approval::${name}`),
  ...['invoke', 'listen', 'optional', 'path', 'primitive_error', 'request']
    .map(name => `binding::members::${name}`),
].map(name => `safe_cex_venue_asterdex::${name}`).sort();
assert.equal(selected.length, 19);
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
  '--document-hidden-items', '-A', 'rustdoc::all']);
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
assert.ok(found.types.every(owner => owner.kind === 'function'));
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-builder-binding-inventory.json'), `${JSON.stringify(found, null, 2)}\n`);
const results = runProbes(workspace, metadata, [{
  ...found, package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-builder-binding-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), selected);
for (const row of results) assert.deepEqual(row.failures, [], row.name);
console.log('Aster builder/binding cohort: 19 compiler owners, zero failed classifications.');
console.log('Scope excludes other owners, aliases, locals, features and wasm; this is not a full package gate.');
