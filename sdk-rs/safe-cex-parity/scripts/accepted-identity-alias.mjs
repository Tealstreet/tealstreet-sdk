import assert from 'node:assert/strict';

// A fixed monomorphization of the compiler's actual identity alias. No caller
// supplies a Rust type expression or selects a replacement owner.
export function identityAliasProbe(alias) {
  const parameters = alias.generics?.params ?? [];
  if (!parameters.length) return undefined;
  const label = `${alias.name}: accepted-set assertion: generic alias requires explicit compiler monomorphization coverage`;
  assert.equal(parameters.length, 1, `${label}: only one direct unbounded identity parameter is supported`);
  const parameter = parameters[0];
  assert.ok(parameter.name && !parameter.name.startsWith("'"), `${label}: expected a named type parameter`);
  assert.deepEqual(alias.generics, {
    params: [{ name: parameter.name, kind: { type: { bounds: [], default: null, is_synthetic: false } } }],
    where_predicates: [],
  }, `${label}: bounds, defaults, lifetime/const/synthetic parameters and where clauses are unsupported`);
  assert.deepEqual(alias.rawTarget, { generic: parameter.name }, `${label}: source must directly name its type parameter`);
  assert.deepEqual(alias.target, { generic: parameter.name }, `${label}: compiler expansion must preserve its type parameter`);
  return {
    suffix: '<::serde_json::Value>',
    canonical: 'serde_json::value::Value',
    monomorphization: 'one fixed serde_json::Value identity-alias probe',
  };
}
