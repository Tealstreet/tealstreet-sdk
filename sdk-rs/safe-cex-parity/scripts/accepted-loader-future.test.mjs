// Compiler snapshot with synthetic artifact context; native controls use Cargo.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { genericAliasProbe } from './accepted-callable-alias.mjs';
import { aliasContract } from './accepted-aliases.mjs';
const snapshot = JSON.parse(fs.readFileSync(new URL('./loader-future-fixture.json', import.meta.url)));
function fixture() {
  const alias = structuredClone(snapshot);
  const contract = { category: 'non-decoder', evidence: 'Fixed compiler alias coverage.', reason: 'Caller-owned task, no decoding or polling.', ...structuredClone({ generics: alias.generics, rawTarget: alias.rawTarget, target: alias.target }) };
  const context = { macroOrigins: ['serde_json'], externalTypeAliases: [
    ['core::pin::Pin', 'struct'], ['alloc::boxed::Box', 'struct'],
    ['core::future::future::Future', 'trait'], ['core::result::Result', 'enum'],
  ].map(([canonical, kind]) => ({ name: alias.name, canonical, kind, artifact: { kind: 'sysroot' } })) };
  return { alias, contract, context };
}
test('loader future uses the actual fixed Value alias and complete expected type', () => {
  const { alias, contract, context } = fixture();
  const probe = genericAliasProbe(alias, contract, context);
  assert.equal(probe.suffix, '<::serde_json::Value>');
  assert.equal(probe.canonical, 'core::pin::Pin');
  assert.equal(probe.expectedType, '::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output=::std::result::Result<::serde_json::Value,crate::poseidon_loaders::LoaderError>>>>');
  assert.equal(aliasContract(alias, { [alias.name]: contract }, context).aliasProbeExpected, probe.expectedType);
});
test('loader future rejects nested output error lifetime and contract redirection', () => {
  const output = row => row.alias.rawTarget.resolved_path.args.angle_bracketed.args[0].type.resolved_path.args.angle_bracketed.args[0].type.dyn_trait;
  const mutations = [
    row => { output(row).traits[0].trait.args.angle_bracketed.constraints[0].binding.equality.type.resolved_path.args.angle_bracketed.args[0].type = { primitive: 'u8' }; },
    row => { output(row).traits[0].trait.args.angle_bracketed.constraints[0].binding.equality.type.resolved_path.args.angle_bracketed.args[1].type.resolved_path.path = 'other::LoaderError'; },
    row => { output(row).lifetime = "'static"; },
    row => { row.alias.generics.params[0].kind.type.default = { primitive: 'u8' }; },
    row => { row.contract.instantiation = 'AnotherAlias'; },
    row => { row.contract.category = 'decoder'; },
    row => { row.contract.rawTarget = { generic: 'T' }; },
    row => { row.contract.target = { generic: 'T' }; },
  ];
  assert.equal(mutations.length, 8);
  for (const mutate of mutations) {
    const row = fixture(); mutate(row);
    assert.throws(() => genericAliasProbe(row.alias, row.contract, row.context), /fixed loader future alias probe/);
  }
  const row = fixture(); row.alias.name = 'other::LoaderFuture';
  assert.throws(() => genericAliasProbe(row.alias, row.contract, row.context), /source must directly name its type parameter/);
  const injected = fixture(); injected.contract.aliasProbeExpected = 'AnotherAlias';
  assert.throws(() => aliasContract(injected.alias, { [injected.alias.name]: injected.contract }, injected.context), /expected compiler type cannot be supplied/);
});
test('loader future requires every actual compiler identity including Result', () => {
  const row = fixture();
  assert.throws(() => genericAliasProbe(row.alias, row.contract), /actual compiler context required/);
  assert.throws(() => genericAliasProbe(row.alias, row.contract, { ...row.context, macroOrigins: [] }), /actual serde_json dependency required/);
  for (const canonical of ['core::pin::Pin', 'alloc::boxed::Box', 'core::future::future::Future', 'core::result::Result']) {
    const context = structuredClone(row.context);
    context.externalTypeAliases = context.externalTypeAliases.filter(item => item.canonical !== canonical);
    assert.throws(() => genericAliasProbe(row.alias, row.contract, context), /actual sysroot .* identity required/);
    context.externalTypeAliases = structuredClone(row.context.externalTypeAliases);
    context.externalTypeAliases.find(item => item.canonical === canonical).artifact.kind = 'workspace';
    assert.throws(() => genericAliasProbe(row.alias, row.contract, context), /actual sysroot .* identity required/);
  }
});
