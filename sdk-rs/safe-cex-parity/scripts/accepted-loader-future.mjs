import assert from 'node:assert/strict';

// One reviewed production alias, not a general generic substitution permission.
export function loaderFutureAliasProbe(alias, contract, context) {
  if (alias.name !== 'safe_cex_crypto::poseidon_loaders::LoaderFuture') return undefined;
  const label = `${alias.name}: accepted-set assertion: fixed loader future alias probe`;
  assert.deepEqual(alias.generics, {
    params: [{ name: 'T', kind: { type: { bounds: [], default: null, is_synthetic: false } } }],
    where_predicates: [],
  }, `${label}: exactly one unbounded output parameter required`);
  const applied = (path, types) => ({ resolved_path: { path, args: {
    angle_bracketed: { args: types.map(type => ({ type })), constraints: [] },
  } } });
  const output = applied('core::result::Result', [
    { generic: 'T' },
    { resolved_path: { path: 'safe_cex_crypto::poseidon_loaders::LoaderError', args: null } },
  ]);
  const expected = applied('core::pin::Pin', [applied('alloc::boxed::Box', [{ dyn_trait: {
    lifetime: null,
    traits: [{ generic_params: [], trait: { path: 'core::future::future::Future', args: {
      angle_bracketed: { args: [], constraints: [{ name: 'Output', args: null,
        binding: { equality: { type: output } } }] },
    } } }],
  } }])]);
  assert.deepEqual(alias.rawTarget, expected, `${label}: exact Result output and LoaderError source required`);
  assert.deepEqual(alias.target, expected, `${label}: expanded target changed`);
  assert.equal(contract?.category, 'non-decoder', `${label}: explicit non-decoder classification required`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: bounded classification evidence required`);
  assert.equal(contract.instantiation, undefined, `${label}: caller cannot redirect the actual alias`);
  assert.deepEqual(contract.generics, alias.generics, `${label}: compiler generics changed`);
  assert.deepEqual(contract.rawTarget, alias.rawTarget, `${label}: compiler source target changed`);
  assert.deepEqual(contract.target, alias.target, `${label}: compiler expanded target changed`);
  assert.ok(Array.isArray(context?.macroOrigins) && Array.isArray(context?.externalTypeAliases), `${label}: actual compiler context required`);
  assert.ok(context.macroOrigins.includes('serde_json'), `${label}: actual serde_json dependency required`);
  for (const [canonical, kind] of [
    ['core::pin::Pin', 'struct'], ['alloc::boxed::Box', 'struct'],
    ['core::future::future::Future', 'trait'], ['core::result::Result', 'enum'],
  ]) {
    const rows = context.externalTypeAliases.filter(row => row.name === alias.name && row.canonical === canonical);
    assert.ok(rows.length && rows.every(row => row.kind === kind && row.artifact?.kind === 'sysroot'), `${label}: actual sysroot ${canonical} identity required`);
  }
  return {
    suffix: '<::serde_json::Value>', canonical: 'core::pin::Pin',
    expectedType: '::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output=::std::result::Result<::serde_json::Value,crate::poseidon_loaders::LoaderError>>>>',
    monomorphization: 'one fixed serde_json::Value actual LoaderFuture alias with Result<Value,LoaderError>; no claim for every T or future execution',
  };
}
