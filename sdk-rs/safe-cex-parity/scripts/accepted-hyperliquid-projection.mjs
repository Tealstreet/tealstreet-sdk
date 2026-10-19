import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const crate = 'safe_cex_venue_hyperliquid';
const packageName = 'safe-cex-venue-hyperliquid';
const trait = `${crate}::ws_ping_lifecycle::WsPingHost`;
const host = `${crate}::host::Host`;
const outputs = {
  Value: { canonical: 'safe_cex_exchange::js_runtime::value::Js', kind: 'enum', crate: 'safe_cex_exchange' },
  Error: { canonical: 'safe_cex_exchange::js_runtime::value::Thrown', kind: 'struct', crate: 'safe_cex_exchange' },
};
const emptyGenerics = { params: [], where_predicates: [] };
const generics = { params: [{ name: 'H', kind: { type: { bounds: [], default: null, is_synthetic: false } } }], where_predicates: [] };
const declarations = Object.fromEntries([
  ['private_class', 'Private'], ['public_class', 'Public'], ['core_exchange_class', 'Exchange'],
].flatMap(([module, prefix]) => ['Value', 'Error'].map(member => [
  `${crate}::${module}::${prefix}${member}`, { module, member, ident: `${prefix}${member}` },
])));
const callerFields = ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical', 'projectionIdentity'];

function expectedIdentity(member) {
  return {
    trait: { canonical: trait, kind: 'trait', file: `${packageName}/src/ws_ping_lifecycle.rs`, generics: emptyGenerics },
    host: { canonical: host, kind: 'struct', file: `${packageName}/src/host/mod.rs`, generics: emptyGenerics },
    output: outputs[member],
    implementation: { file: `${packageName}/src/host/host_core.rs`, generics: emptyGenerics,
      is_unsafe: false, is_negative: false, is_synthetic: false, blanket_impl: null },
    binding: { name: member, generics: emptyGenerics, bounds: [], file: `${packageName}/src/host/host_core.rs` },
  };
}

// This evidence is extracted from the selected rustdoc index, before IDs are
// normalized away. A contract never supplies an implementation or binding.
export function hyperliquidProjectionCompilerIdentity(doc, item, paths) {
  const name = paths.get(String(item.id))?.join('::');
  const declaration = declarations[name];
  if (!declaration) return undefined;
  const label = `${name}: accepted-set assertion: associated projection compiler identity`;
  const get = id => doc.index[String(id)];
  function root(id, kind) {
    const value = get(id), canonical = doc.paths[String(id)];
    assert.ok(value?.crate_id === 0 && canonical?.crate_id === 0 && canonical.kind === kind
      && value.inner[kind] && paths.get(String(id))?.join('::') === canonical.path.join('::'), `${label}: actual same-crate ${kind} root required`);
    return { canonical: canonical.path.join('::'), kind, file: value.span?.filename, generics: value.inner[kind].generics };
  }
  const projection = item.inner.type_alias.type.qualified_path;
  assert.ok(projection?.trait?.id != null, `${label}: actual associated trait required`);
  const traitRoot = root(projection.trait.id, 'trait');
  assert.equal(traitRoot.canonical, trait, `${label}: actual WsPingHost root required`);
  const realm = Object.values(doc.index).filter(value => value.crate_id === 0 && value.inner.struct
    && paths.get(String(value.id))?.join('::') === host);
  assert.equal(realm.length, 1, `${label}: unique Host declaration required`);
  const hostRoot = root(realm[0].id, 'struct');
  const implementations = Object.values(doc.index).filter(value => value.crate_id === 0 && value.inner.impl
    && String(value.inner.impl.trait?.id) === String(projection.trait.id)
    && String(value.inner.impl.for.resolved_path?.id) === String(realm[0].id));
  assert.equal(implementations.length, 1, `${label}: unique actual Host implementation required`);
  const implementation = implementations[0], impl = implementation.inner.impl;
  assert.ok(realm[0].inner.struct.impls.map(String).includes(String(implementation.id))
    && get(projection.trait.id).inner.trait.implementations.map(String).includes(String(implementation.id)), `${label}: compiler implementation ownership required`);
  assert.equal(impl.trait.args, null, `${label}: plain host trait required`);
  assert.equal(impl.for.resolved_path.args, null, `${label}: concrete Host required`);
  const declaredMembers = get(projection.trait.id).inner.trait.items.map(get)
    .filter(value => value?.name === declaration.member && value.inner.assoc_type);
  const bindings = impl.items.map(get).filter(value => value?.name === declaration.member && value.inner.assoc_type);
  assert.equal(declaredMembers.length, 1, `${label}: declared associated member required`);
  assert.equal(bindings.length, 1, `${label}: unique associated binding required`);
  const binding = bindings[0], associated = binding.inner.assoc_type;
  assert.equal(binding.crate_id, 0, `${label}: same-crate associated binding required`);
  assert.deepEqual(Object.keys(associated.type), ['resolved_path'], `${label}: direct associated output required`);
  assert.equal(associated.type.resolved_path.args, null, `${label}: concrete output required`);
  function outputRoot(id) {
    const actual = doc.paths[String(id)];
    const dependency = doc.external_crates?.[String(actual?.crate_id)];
    const expected = outputs[declaration.member];
    assert.ok(actual && actual.crate_id !== 0 && dependency?.name === expected.crate
      && actual.path.join('::') === expected.canonical && actual.kind === expected.kind
      && typeof dependency.path === 'string' && dependency.path.length > 0
      && !paths.has(String(id)),
      `${label}: exact shared compiler output root required`);
    return { canonical: actual.path.join('::'), kind: actual.kind, crate: dependency.name };
  }
  const identity = {
    trait: traitRoot, host: hostRoot, output: outputRoot(associated.type.resolved_path.id),
    implementation: { file: implementation.span?.filename, generics: impl.generics,
      is_unsafe: impl.is_unsafe, is_negative: impl.is_negative, is_synthetic: impl.is_synthetic, blanket_impl: impl.blanket_impl },
    binding: { name: binding.name, generics: associated.generics, bounds: associated.bounds, file: binding.span?.filename },
  };
  assert.deepEqual(identity, expectedIdentity(declaration.member), `${label}: reviewed Host associated identity changed`);
  return identity;
}

export function reviewedHyperliquidProjectionProbe(alias, contract, context, spec) {
  const declaration = declarations[alias.name];
  if (!declaration) return undefined;
  const label = `${alias.name}: accepted-set assertion: reviewed associated projection`;
  assert.equal(alias.ident, declaration.ident, `${label}: compiler identifier changed`);
  assert.equal(alias.span?.filename, `${packageName}/src/${declaration.module}.rs`, `${label}: compiler source identity changed`);
  const target = { qualified_path: { name: declaration.member, args: null, self_type: { generic: 'H' }, trait: { path: trait, args: null } } };
  assert.deepEqual(alias.generics, generics, `${label}: exact unbounded host parameter required`);
  assert.deepEqual(alias.rawTarget, target, `${label}: exact raw associated projection required`);
  assert.deepEqual(alias.target, target, `${label}: exact expanded associated projection required`);
  assert.deepEqual(alias.projectionIdentity, expectedIdentity(declaration.member), `${label}: actual compiler associated identity required`);
  assert.equal(spec.canonical, outputs[declaration.member].canonical, `${label}: reviewed concrete output root required`);
  assert.equal(spec.expectedType, outputs[declaration.member].canonical, `${label}: policy-owned native expected type required`);
  assert.equal(spec.suffix, '<crate::host::Host>', `${label}: only the fixed Host host is reviewed`);
  assert.equal(spec.unsized, false, `${label}: concrete output is sized`);
  for (const key of callerFields) assert.equal(contract?.[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler dependency context required`);
  assert.equal(context.packageMetadata?.name, packageName, `${label}: actual selected package required`);
  assert.equal(fs.realpathSync(context.packageMetadata.manifest_path), fs.realpathSync(path.join(context.workspace, packageName, 'Cargo.toml')), `${label}: selected workspace source required`);
  return { suffix: spec.suffix, canonical: spec.canonical, expectedType: spec.expectedType, probeKind: 'reviewed-fixed',
    monomorphization: `one reviewed ${alias.name}<Host> associated projection; ordinary decoder and conversion probes remain required` };
}
