import assert from 'node:assert/strict';

// One compiler-confirmed family, not a general lifetime substitution engine.
export function localFutureAliasProbe(alias, contract, context) {
  if (alias.rawTarget?.resolved_path?.path !== 'core::pin::Pin' || alias.generics?.params?.length !== 2) return undefined;
  const label = `${alias.name}: accepted-set assertion: fixed local future alias probe`;
  const params = alias.generics?.params ?? [];
  assert.equal(params.length, 2, `${label}: one lifetime and one unbounded output parameter required`);
  const [lifetime, output] = params;
  assert.ok(lifetime.name?.startsWith("'") && output.name && !output.name.startsWith("'"), `${label}: lifetime then output parameter required`);
  assert.deepEqual(alias.generics, {
    params: [
      { name: lifetime.name, kind: { lifetime: { outlives: [] } } },
      { name: output.name, kind: { type: { bounds: [], default: null, is_synthetic: false } } },
    ], where_predicates: [],
  }, `${label}: bounds/defaults/const/where clauses unsupported`);
  const applied = (path, type) => ({ resolved_path: { path, args: { angle_bracketed: { args: [{ type }], constraints: [] } } } });
  const associatedOutput = { name: 'Output', args: null,
    binding: { equality: { type: { generic: output.name } } } };
  const futureTrait = {
    trait: { path: 'core::future::future::Future', args: {
      angle_bracketed: { args: [], constraints: [associatedOutput] },
    } }, generic_params: [],
  };
  const expected = applied('core::pin::Pin', applied('alloc::boxed::Box', {
    dyn_trait: { traits: [futureTrait], lifetime: lifetime.name },
  }));
  assert.deepEqual(alias.rawTarget, expected, `${label}: source must be Pin<Box<dyn Future<Output=T>+'a>>`);
  assert.deepEqual(alias.target, expected, `${label}: expanded target changed`);
  assert.equal(contract?.category, 'non-decoder', `${label}: explicit non-decoder classification required`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: bounded classification evidence required`);
  assert.equal(contract.instantiation, undefined, `${label}: caller cannot redirect the actual alias`);
  assert.deepEqual(contract.generics, alias.generics, `${label}: compiler generics changed`);
  assert.deepEqual(contract.rawTarget, alias.rawTarget, `${label}: compiler source target changed`);
  assert.deepEqual(contract.target, alias.target, `${label}: compiler expanded target changed`);
  assert.ok(Array.isArray(context?.macroOrigins) && Array.isArray(context?.externalTypeAliases), `${label}: actual compiler context required`);
  assert.ok(context.macroOrigins.includes('serde_json'), `${label}: actual serde_json dependency required`);
  for (const [canonical, kind] of [['core::pin::Pin', 'struct'], ['alloc::boxed::Box', 'struct'], ['core::future::future::Future', 'trait']]) {
    const rows = context.externalTypeAliases.filter(row => row.name === alias.name && row.canonical === canonical);
    assert.ok(rows.length && rows.every(row => row.kind === kind && row.artifact?.kind === 'sysroot'), `${label}: actual sysroot ${canonical} identity required`);
  }
  return {
    suffix: "<'static,::serde_json::Value>", canonical: 'core::pin::Pin',
    monomorphization: "one fixed 'static/serde_json::Value actual local-future alias probe; no claim for every lifetime or T",
  };
}
