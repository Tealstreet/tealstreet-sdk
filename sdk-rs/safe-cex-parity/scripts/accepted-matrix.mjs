import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { policy, validateCoverage } from '../../../.github/scripts/rust-wasm.mjs';
import { checkRatchet, checkHistory } from './accepted-ratchet.mjs';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(scripts, '../../..');
const root = path.join(repo, 'sdk-rs');
const metadataRun = spawnSync('cargo', ['metadata', '--no-deps', '--locked', '--format-version', '1'], { cwd: root, encoding: 'utf8' });
assert.equal(metadataRun.status, 0, metadataRun.stderr);
const metadata = JSON.parse(metadataRun.stdout);
validateCoverage(metadata, policy);
const members = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id));
const results = [];
const declarations = new Map();
function run(args) {
  const result = spawnSync(process.execPath, [path.join(scripts, 'accepted-set.mjs'), '--measure', ...args], { stdio: 'inherit' });
  assert.equal(result.status, 0, 'accepted-set: configuration measurement must complete');
  for (const pkg of JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'accepted-set-inventory.json'), 'utf8'))) {
    for (const owner of pkg.types) if (owner.kind !== 'function') declarations.set(owner.name, path.relative(repo, path.resolve(root, owner.span.filename)));
  }
  results.push(...JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'accepted-set-results.json'), 'utf8')));
}
run([]);
for (const pkg of members) {
  const features = Object.keys(pkg.features).filter(name => name !== 'default');
  assert.ok(features.length <= 8, `${pkg.name}: feature matrix requires review before exponential expansion`);
  assert.deepEqual(pkg.features.default ?? [], [], `${pkg.name}: nonempty default features need explicit off/on coverage`);
  for (let mask = 1; mask < 2 ** features.length; mask++) {
    const enabled = features.filter((_, i) => mask & (2 ** i));
    // This binding deliberately rejects its browser feature on a native target.
    if (pkg.name === 'safe-cex-ffi' && enabled.includes('wasm')) continue;
    run([`--package=${pkg.name}`, `--features=${enabled.join(',')}`]);
  }
  if (!policy.wasm.some(entry => entry.package === pkg.name)) continue;
  for (let mask = 0; mask < 2 ** features.length; mask++) {
    const enabled = features.filter((_, i) => mask & (2 ** i));
    // napi's Node ABI is native-only; its wasm binding is separately exercised.
    if (pkg.name === 'safe-cex-ffi' && enabled.includes('node')) continue;
    run([`--package=${pkg.name}`, '--target=wasm32-unknown-unknown', `--features=${enabled.join(',')}`]);
  }
}
fs.writeFileSync(path.join(metadata.target_directory, 'accepted-set-matrix-results.json'), `${JSON.stringify(results, null, 2)}\n`);
const relative = 'sdk-rs/safe-cex-parity/accepted-set/outstanding-defects.json';
const debt = JSON.parse(fs.readFileSync(path.join(repo, relative), 'utf8'));
checkHistory(repo, relative, debt, { declarations });
const count = checkRatchet(results, debt);
console.log(`accepted-set: enforced ${new Set(results.filter(row => !row.classificationOnly).map(row => row.name)).size} runtime owners and ${new Set(results.filter(row => row.classificationOnly).map(row => row.name)).size} explicit classifications; ${count} owned outstanding defects; no new or stale failures`);
