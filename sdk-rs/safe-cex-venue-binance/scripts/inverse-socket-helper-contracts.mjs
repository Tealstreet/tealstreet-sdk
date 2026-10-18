// Reviewed runtime-helper batch only; this is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const args=process.argv.slice(2);
assert.ok(args.length<=1 && args.every(a=>a==='--expect-unclassified' || a.startsWith('--only=')));
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
// Keep proc-macro artifact configuration consistent through discovery and probes.
process.env.RUSTC_BOOTSTRAP = '1';
const selected = [
 ...['call','flag','global','optional','send','timeout'].map(n=>'ws_private::'+n),
 ...['call','flag','global','subscription','tickers'].map(n=>'ws_public::'+n),
].map(name => `safe_cex_venue_binance::inverse::${name}`).sort();
const chosen=args[0]?.startsWith('--only=')?args[0].slice(7).split(','):selected;
assert.equal(selected.length,11);
assert.ok(chosen.length && new Set(chosen).size===chosen.length && chosen.every(n=>selected.includes(n)));
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extraEnv },
  });
  assert.equal(result.status, 0, `${args.join(' ')}: ${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(item => item.name === 'safe-cex-venue-binance');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(item => item.kind.includes('lib'));
assert.ok(target);
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1', CARGO_TARGET_DIR: path.join(metadata.target_directory, 'accepted-inventory') });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'accepted-inventory/doc', `${target.name}.json`)));
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
fs.writeFileSync(path.join(metadata.target_directory, 'inverse-socket-helper-owners.json'), JSON.stringify(found.types, null, 2));
const contracts = loadContracts();
const results = runProbes(workspace, metadata, [{
  ...found, types: found.types.filter(t=>chosen.includes(t.name)), package: pkg.name, packageId: pkg.id, target, features: [],
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'inverse-socket-helper-contract-results.json'), `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(row => row.name).sort(), chosen.slice().sort());
if(args[0]==='--expect-unclassified') {
 assert.ok(results.every(r=>r.failures.length && r.failures.some(f=>/unclassified|needs an accepted-set contract/.test(f))));
 console.log('All11 socket owners produce compiled missing-classification RED receipts.');
} else {
 for (const row of results) {
  assert.deepEqual(row.failures, [], row.name);
  assert.ok(!row.classificationOnly, `${row.name}: actual native execution required`);
 }
 console.log(`Inverse socket helpers: ${results.length} real native owners pass. Scope excludes other owners.`);
}
