import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
// Run alone: the accepted-set sandbox and temporary production mutations are serial.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
function cargo(args) {
  const result = spawnSync('cargo', ['--color', 'never', ...args], {
    cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1' },
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(row => row.name === 'safe-cex-venue-weex');
const target = pkg.targets.find(row => row.kind.includes('lib'));
cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs=1', '--', '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all']);
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
const found = inventory(doc, {
  defaultTraits: loadPolicy('default-traits'),
  receiverTraits: loadPolicy('receiver-traits'),
  readSource: file => fs.readFileSync(path.join(root, file)),
});
const contracts = loadContracts();
const file = path.join(root, 'safe-cex-venue-weex/src/binding/values.rs');
const original = fs.readFileSync(file);
const controls = [
  ['capture', '    to_typed(value)\n}', '    let _ = value; to_typed(&Js::Null)\n}'],
  ['maybe_capture', '    if value.is_undefined() {\n        Ok(None)', '    if true {\n        Ok(None)'],
  ['normalize', 'utils::normalize_order_side(input)', 'utils::normalize_order_status(input)'],
];
const names = controls.map(([name]) => `safe_cex_venue_weex::binding::values::${name}`);
const types = found.types.filter(row => names.includes(row.name));
assert.equal(types.length, 3);
function run(selected) {
  return runProbes(root, metadata, [{ package: pkg.name, target, types: selected, publicAliases: [], locals: [] }], contracts, { target: 'native', features: [], jobs: 1 });
}
assert.ok(run(types).every(row => !row.failures.length));
for (const [name, before, after] of controls) {
  const owner = types.find(row => row.name.endsWith(`::${name}`));
  const source = original.toString();
  assert.equal(source.split(before).length, 2, `${name}: unique mutation target`);
  try {
    fs.writeFileSync(file, source.replace(before, after));
    const receipts = run([owner]);
    assert.equal(receipts.length, 1);
    assert.equal(receipts[0].name, owner.name);
    assert.ok(receipts[0].failures.length, `${name}: compiled production mutation must fail its native contract`);
    console.log(`CONTROL ${name}: compiled native contract rejected mutation`);
  } finally {
    fs.writeFileSync(file, original);
    assert.deepEqual(fs.readFileSync(file), original);
  }
  assert.ok(run([owner]).every(row => !row.failures.length), `${name}: restored native contract`);
  console.log(`RESTORED ${name}: native contract passes`);
}
