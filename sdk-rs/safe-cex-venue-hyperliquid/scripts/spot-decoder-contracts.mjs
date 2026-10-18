// Execute the real public Spot decoders and their delegated OnlyMap visitors.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
import { verifiedMacroOrigins } from '../../safe-cex-parity/scripts/accepted-macro-origins.mjs';
import { buildScanner } from '../../safe-cex-parity/scripts/accepted-scanner.mjs';
import { requireNativeDecoderObservations } from '../../safe-cex-parity/scripts/accepted-local-helpers.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const owners = ['WsFill', 'WsLiquidation', 'WsNonUserCancel', 'WsUserFunding']
  .map(name => `safe_cex_venue_hyperliquid::spot_models::${name}`);
const missing = process.argv.includes('--expect-missing');
assert.deepEqual(process.argv.slice(2), missing ? ['--expect-missing'] : []);
const target = path.join(root, 'target/accepted-inventory');
function run(command, args, extra = {}) {
  const result = spawnSync(command, args, {
    cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_INCREMENTAL: '0', CARGO_BUILD_JOBS: '1', ...extra },
  });
  assert.equal(result.status, 0, `Spot compiler preparation failed; not a semantic control\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--format-version=1']));
const packages = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id)
  && pkg.name === 'safe-cex-venue-hyperliquid');
assert.equal(packages.length, 1);
const pkg = packages[0];
const targets = pkg.targets.filter(item => item.kind.includes('lib'));
assert.equal(targets.length, 1);
const library = targets[0];
const scanner = buildScanner(1);
run(scanner, ['--doc-guards', library.src_path]);
run('cargo', ['rustdoc', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'],
{ CARGO_TARGET_DIR: target, RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(target, 'doc', `${library.name}.json`)));
const artifacts = run('cargo', ['check', '--locked', '-p', pkg.name, '--jobs', '1',
  '--lib', '--message-format=json'], { CARGO_TARGET_DIR: target, RUSTC_BOOTSTRAP: '1' }).trim().split('\n')
  .filter(Boolean).map(line => JSON.parse(line)).filter(row => row.reason === 'compiler-artifact');
const own = artifacts.filter(row => row.package_id === pkg.id && row.target.name === library.name);
assert.equal(own.length, 1, 'fresh production library artifact');
const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, run('rustc', ['--print', 'sysroot']).trim());
assert.ok(macroOrigins.includes('serde_json'), 'authenticate the actual serde_json dependency');
const found = inventory(doc, { defaultTraits: loadPolicy('default-traits'),
  receiverTraits: loadPolicy('receiver-traits'),
  readSource: file => fs.readFileSync(path.resolve(root, file)),
});
const types = found.types.filter(owner => owners.includes(owner.name));
assert.deepEqual(types.map(owner => owner.name).sort(), owners);
const source = path.join(root, 'safe-cex-venue-hyperliquid/src/spot_models.rs');
for (const owner of types) {
  assert.equal(owner.deserialize, true);
  assert.equal(fs.realpathSync(path.resolve(root, owner.span.filename)), fs.realpathSync(source));
  assert.deepEqual(owner.generics.params, []);
}
const contracts = loadContracts();
const lines = run(scanner, ['--macro-origins-json', JSON.stringify(macroOrigins), source])
  .trim().split('\n').filter(Boolean);
const locals = [];
const delegatedLocals = [];
for (const line of lines) {
  const [file, ident, row, column] = line.split('\t');
  assert.equal(fs.realpathSync(file), fs.realpathSync(source));
  const match = /^LOCAL_TRAIT_IMPL:serde::de::Visitor:OnlyMap@(\w+)$/.exec(ident);
  const label = `${path.relative(root, file)}::${ident}`;
  if (match) {
    const matches = types.filter(owner => owner.ident === match[1]);
    assert.equal(matches.length, 1, 'unique actual enclosing Deserialize owner');
    const owner = matches[0];
    if (missing) {
      assert.equal(contracts[owner.name], undefined);
      assert.throws(() => requireNativeDecoderObservations(owner, undefined, label),
        /decoder helper needs direct native observations/);
      console.log(`${label}: stock decoder-helper guard RED: missing native observations`);
    } else requireNativeDecoderObservations(owner, contracts[owner.name], label);
    delegatedLocals.push({ name: `${label}@${row}:${column}`, runtimeOwner: owner.name });
  } else {
    assert.equal(ident, 'OnlyMap', 'do not omit an unrecognized local scope');
    locals.push({ file, ident, line: Number(row), column: Number(column), label: `${label}@${row}:${column}` });
  }
}
assert.equal(delegatedLocals.length, 4);
assert.equal(locals.length, 4);
const receipts = runProbes(root, metadata, [{ package: pkg.name, packageId: pkg.id,
  target: library, features: own[0].features, types, macroOrigins,
  locals, delegatedLocals: missing ? [] : delegatedLocals,
}], contracts, { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'hl-spot-decoder-results.json'),
  `${JSON.stringify(receipts, null, 2)}\n`);
const native = receipts.filter(row => owners.includes(row.name));
assert.deepEqual(native.map(row => row.name).sort(), owners);
const localReceipts = receipts.filter(row => row.local === true);
assert.deepEqual(localReceipts.map(row => row.name).sort(), locals.map(row => row.label).sort());
for (const row of localReceipts) assert.deepEqual(row.failures, [], row.name);
if (missing) {
  for (const row of native) assert.ok(row.failures.some(failure =>
    failure.includes('unclassified deserializable type')), JSON.stringify(row));
  console.log('SPOT_DECODER_MISSING: four actual native owners fail absent classification');
  process.exit(0);
}
for (const row of native) {
  assert.notEqual(row.classificationOnly, true, row.name);
  assert.deepEqual(row.failures, [], `SPOT_DECODER_NATIVE ${row.name}`);
}
const visitors = receipts.filter(row => row.category === 'private-decoder-helper'
  && row.name.includes('spot_models.rs::LOCAL_TRAIT_IMPL:serde::de::Visitor:OnlyMap@'));
assert.equal(visitors.length, 4, 'all four real OnlyMap visitors must delegate');
assert.deepEqual(visitors.map(row => row.runtimeOwner).sort(), owners);
for (const row of visitors) assert.deepEqual(row.failures, [], row.name);
assert.equal(receipts.length, 12, 'four native owners, four local structs, four delegated visitors');
console.log(`SPOT_DECODER_NATIVE: ${native.length} executed owners; ${visitors.length} delegated visitors`);
