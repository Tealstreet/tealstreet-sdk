// Four individually reviewed private request-record owners; other obligations remain open.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env },
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
assert.equal(process.argv.length, 2, 'unsupported arguments');
const selected = [
  'safe_cex_venue_kraken::futures_candles::Options',
  'safe_cex_venue_kraken::reads::AccountLogParams',
  'safe_cex_venue_kraken::reads::HistoricalOptions',
  'safe_cex_venue_kraken::spot_reads::Options',
];
const companions = [
  'safe_cex_venue_kraken::futures_candles::FuturesOptionsWire',
  'safe_cex_venue_kraken::reads::AccountLogParamsWire',
  'safe_cex_venue_kraken::reads::HistoricalOptionsWire',
  'safe_cex_venue_kraken::spot_reads::SpotOptionsWire',
];
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(value => value.name === 'safe-cex-venue-kraken');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual Kraken compiler package required');
const target = pkg.targets.find(value => value.kind.includes('lib'));
assert.ok(target, 'actual production library required');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_kraken.json'), 'utf8'));
const found = inventory(doc, {
  onlyOwners: selected,
  readSource: file => fs.readFileSync(path.join(workspace, file)),
});
const types = found.types;
assert.deepEqual(types.map(value => value.name).sort(), selected);
assert.ok(types.every(value => value.deserialize && value.kind === 'record'));
// A remote companion has no decoder obligation, so it cannot be requested as
// an inventory owner. Inspect every actual nonblanket implementation instead;
// new traits, conversions and inherent APIs must refuse rather than disappear.
for (const [position, name] of companions.entries()) {
  const ident = name.split('::').at(-1);
  const matches = Object.values(doc.index).filter(item => item.crate_id === 0 && item.name === ident && item.inner.struct);
  assert.equal(matches.length, 1, `${name}: unique actual compiler companion required`);
  const companion = matches[0];
  const owner = types.find(value => value.name === selected[position]);
  assert.notEqual(companion.visibility, 'public', `${name}: private companion required`);
  const methods = [];
  for (const id of companion.inner.struct.impls) {
    const impl = doc.index[String(id)]?.inner.impl;
    if (!impl || impl.is_synthetic || impl.is_negative || impl.blanket_impl) continue;
    assert.equal(String(impl.for?.resolved_path?.id), String(companion.id));
    assert.equal(impl.trait, null, `${name}: new trait/conversion/Deserialize obligation requires review`);
    methods.push(...impl.items.map(id => doc.index[String(id)]));
  }
  assert.equal(methods.length, 1, `${name}: unexpected additional inherent API`);
  const method = methods[0];
  assert.equal(method.name, 'deserialize');
  assert.notEqual(method.visibility, 'public', `${name}: public entrypoint discovered`);
  const fn = method.inner.function;
  assert.ok(fn, `${name}: expected generated remote decoder`);
  assert.equal(fn.sig.inputs.length, 1);
  const parameter = fn.sig.inputs[0][1].generic;
  assert.equal(typeof parameter, 'string', `${name}: unexpected raw decoder input`);
  assert.equal(fn.generics.params.length, 1);
  assert.equal(fn.generics.params[0].name, parameter);
  assert.ok(fn.generics.where_predicates.some(predicate => {
    const bound = predicate.bound_predicate;
    return bound?.type?.generic === parameter && bound.bounds.some(value => {
      const trait = doc.paths[String(value.trait_bound?.trait?.id)]?.path.join('::');
      return ['serde::de::Deserializer', 'serde_core::de::Deserializer'].includes(trait);
    });
  }), `${name}: existing Serde deserializer boundary required`);
  const result = fn.sig.output?.resolved_path?.args?.angle_bracketed?.args?.[0]?.type?.resolved_path;
  const returnedOwner = doc.index[String(result?.id)];
  assert.equal(returnedOwner?.name, owner.ident, `${name}: remote decoder must return the actual reviewed owner`);
  assert.deepEqual(returnedOwner?.span, owner.span, `${name}: returned owner compiler identity changed`);
}
console.log('Selected compiler inventory: four actual Deserialize records; all four private remote companions have no independent Deserialize/public entrypoint. Every nonblanket companion implementation is inspected; new traits, conversions or inherent APIs require review.');
console.log('Probe scope: four native/default record owners only. Whole-crate receiver refusals, other owners, aliases, locals, construction and feature/wasm coverage remain open. Run accepted probes serially.');
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
for (const result of results) assert.equal(result.failures.length, 0,
  `${result.name}: ${result.failures.join('; ')}`);
assert.ok(results.every(result => !result.classificationOnly), 'actual runtime production probe required');
console.log('Four request-record production probes passed; zero failures. Other Kraken owners remain open.');
