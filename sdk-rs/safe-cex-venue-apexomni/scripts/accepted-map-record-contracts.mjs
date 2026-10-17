// Twenty-six individually reviewed map-record owners; other obligations remain open.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { contractFor } from '../../safe-cex-parity/scripts/accepted-contracts.mjs';
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
  "safe_cex_venue_apexomni::config_records::MultiChainObject",
  "safe_cex_venue_apexomni::config_records::OrderObject",
  "safe_cex_venue_apexomni::config_records::PerpetualObject",
  "safe_cex_venue_apexomni::config_records::SymbolInfoObject",
  "safe_cex_venue_apexomni::config_records::SymbolObject",
  "safe_cex_venue_apexomni::config_records::UserObject",
  "safe_cex_venue_apexomni::private_records::AccountBalanceObject",
  "safe_cex_venue_apexomni::private_records::AccountObject",
  "safe_cex_venue_apexomni::private_records::AccountsItem",
  "safe_cex_venue_apexomni::private_records::FundingRateObject",
  "safe_cex_venue_apexomni::private_records::HistoricalPNLObject",
  "safe_cex_venue_apexomni::private_records::OpenPositionObject",
  "safe_cex_venue_apexomni::private_records::WalletObject",
  "safe_cex_venue_apexomni::private_records::WorstPriceObject",
  "safe_cex_venue_apexomni::public_records::ApiKeyCredentials",
  "safe_cex_venue_apexomni::public_records::CurrencyObject",
  "safe_cex_venue_apexomni::public_records::DepthObject",
  "safe_cex_venue_apexomni::public_records::GetHistoryFundingOptions",
  "safe_cex_venue_apexomni::public_records::GlobalObject",
  "safe_cex_venue_apexomni::public_records::HistoryFundingObject",
  "safe_cex_venue_apexomni::public_records::KeyPair",
  "safe_cex_venue_apexomni::public_records::KlineObject",
  "safe_cex_venue_apexomni::public_records::PerpetualContractObject",
  "safe_cex_venue_apexomni::public_records::PerpetualCurrencyObject",
  "safe_cex_venue_apexomni::public_records::TickerObject",
  "safe_cex_venue_apexomni::public_records::TradeObject"
];
const companions = [
  "safe_cex_venue_apexomni::config_records::MultiChainObjectWire",
  "safe_cex_venue_apexomni::config_records::OrderObjectWire",
  "safe_cex_venue_apexomni::config_records::PerpetualObjectWire",
  "safe_cex_venue_apexomni::config_records::SymbolInfoObjectWire",
  "safe_cex_venue_apexomni::config_records::SymbolObjectWire",
  "safe_cex_venue_apexomni::config_records::UserObjectWire",
  "safe_cex_venue_apexomni::private_records::AccountBalanceObjectWire",
  "safe_cex_venue_apexomni::private_records::AccountObjectWire",
  "safe_cex_venue_apexomni::private_records::AccountsItemWire",
  "safe_cex_venue_apexomni::private_records::FundingRateObjectWire",
  "safe_cex_venue_apexomni::private_records::HistoricalPNLObjectWire",
  "safe_cex_venue_apexomni::private_records::OpenPositionObjectWire",
  "safe_cex_venue_apexomni::private_records::WalletObjectWire",
  "safe_cex_venue_apexomni::private_records::WorstPriceObjectWire",
  "safe_cex_venue_apexomni::public_records::ApiKeyCredentialsWire",
  "safe_cex_venue_apexomni::public_records::CurrencyObjectWire",
  "safe_cex_venue_apexomni::public_records::DepthObjectWire",
  "safe_cex_venue_apexomni::public_records::GetHistoryFundingOptionsWire",
  "safe_cex_venue_apexomni::public_records::GlobalObjectWire",
  "safe_cex_venue_apexomni::public_records::HistoryFundingObjectWire",
  "safe_cex_venue_apexomni::public_records::KeyPairWire",
  "safe_cex_venue_apexomni::public_records::KlineObjectWire",
  "safe_cex_venue_apexomni::public_records::PerpetualContractObjectWire",
  "safe_cex_venue_apexomni::public_records::PerpetualCurrencyObjectWire",
  "safe_cex_venue_apexomni::public_records::TickerObjectWire",
  "safe_cex_venue_apexomni::public_records::TradeObjectWire"
];
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(value => value.name === 'safe-cex-venue-apexomni');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual ApexOmni compiler package required');
const target = pkg.targets.find(value => value.kind.includes('lib'));
assert.ok(target, 'actual production library required');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_apexomni.json'), 'utf8'));
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
console.log('Selected compiler inventory: 26 actual Deserialize records; all 26 private remote companions have no independent Deserialize/public entrypoint. Every nonblanket companion implementation is inspected; new traits, conversions or inherent APIs require review.');
console.log('Probe scope: 26 native/default record owners only. Whole-crate receiver refusals, other owners, aliases, locals, construction and feature/wasm coverage remain open. Run accepted probes serially.');
const contracts = loadContracts();
for (const type of types) contractFor(type, contracts);
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], contracts, { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
for (const result of results) assert.equal(result.failures.length, 0,
  `${result.name}: ${result.failures.join('; ')}`);
assert.ok(results.every(result => !result.classificationOnly), 'actual runtime production probe required');
console.log('26 map-record production probes passed; zero failures. Other ApexOmni owners remain open.');
