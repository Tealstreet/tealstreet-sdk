import assert from 'node:assert/strict';
import { identityAliasProbe } from './accepted-identity-alias.mjs';
import { localFutureAliasProbe } from './accepted-future-alias.mjs';
import { loaderFutureAliasProbe } from './accepted-loader-future.mjs';
import { exchangeFutureAliasProbe } from './accepted-exchange-future.mjs';

const PREFIX = 'accepted-set assertion: fixed callable alias probe';
function callableRoot(type) {
  const root = type?.resolved_path;
  if (root?.path !== 'alloc::rc::Rc') return undefined;
  const args = root.args?.angle_bracketed;
  if (!args || args.constraints?.length !== 0 || args.args?.length !== 1) return undefined;
  const dyn = args.args[0].type?.dyn_trait;
  if (!dyn || dyn.lifetime !== null || dyn.traits?.length !== 1) return undefined;
  const only = dyn.traits[0];
  if (only.generic_params?.length !== 0 || only.trait?.path !== 'core::ops::function::Fn') return undefined;
  const signature = only.trait.args?.parenthesized;
  if (!signature || !Array.isArray(signature.inputs) || !signature.output) return undefined;
  return root;
}
function boundedAst(node, parameter) {
  if (node === null || typeof node !== 'object') return;
  if (Array.isArray(node)) { for (const value of node) boundedAst(value, parameter); return; }
  // This initial family does not prove binder substitution or borrowed types.
  if (Object.hasOwn(node, 'generic')) assert.equal(node.generic, parameter, `${PREFIX}: unrelated generic`);
  if (node.dyn_trait) assert.equal(node.dyn_trait.traits?.length, 1, `${PREFIX}: multiple dyn bounds unsupported`);
  if (Object.hasOwn(node, 'generic_params')) assert.deepEqual(node.generic_params, [], `${PREFIX}: higher-ranked parameters unsupported`);
  if (Object.hasOwn(node, 'lifetime')) assert.equal(node.lifetime, null, `${PREFIX}: explicit lifetimes unsupported`);
  for (const value of Object.values(node)) boundedAst(value, parameter);
}
export function genericAliasProbe(alias, contract, context) {
  if (!(alias.generics?.params ?? []).length) return undefined;
  const exchange = exchangeFutureAliasProbe(alias, contract, context);
  if (exchange) return exchange;
  const loader = loaderFutureAliasProbe(alias, contract, context);
  if (loader) return loader;
  const future = localFutureAliasProbe(alias, contract, context);
  if (future) return future;
  // Other generic shapes still use the original, unchanged fail-closed path.
  if (!callableRoot(alias.rawTarget) || !callableRoot(alias.target)) {
    const identity = identityAliasProbe(alias);
    const label = `${alias.name}: accepted-set assertion: fixed identity alias probe`;
    assert.ok(Array.isArray(context?.macroOrigins), `${label}: actual compiler origin context required`);
    assert.ok(context.macroOrigins.includes('serde_json'), `${label}: actual serde_json dependency required`);
    // Plain I has no resolved external target path. Do not invent Rc/Fn or
    // serde_json externalTypeAliases records to authenticate the fixed value.
    return identity;
  }
  const label = `${alias.name}: ${PREFIX}`;
  const parameters = alias.generics.params;
  assert.equal(parameters.length, 1, `${label}: exactly one unbounded type parameter required`);
  const parameter = parameters[0];
  assert.ok(parameter.name && !parameter.name.startsWith("'"), `${label}: named type parameter required`);
  assert.deepEqual(alias.generics, { params: [{ name: parameter.name, kind: { type: { bounds: [], default: null, is_synthetic: false } } }], where_predicates: [] }, `${label}: bounds/defaults/lifetimes/const/where clauses unsupported`);
  boundedAst(alias.rawTarget, parameter.name);
  boundedAst(alias.target, parameter.name);
  assert.equal(contract?.category, 'non-decoder', `${label}: only explicit non-decoder classification supported`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: bounded classification evidence required`);
  assert.equal(contract.instantiation, undefined, `${label}: caller cannot redirect the production alias`);
  assert.deepEqual(contract.generics, alias.generics, `${label}: compiler generics changed`);
  assert.deepEqual(contract.rawTarget, alias.rawTarget, `${label}: compiler source target changed`);
  assert.deepEqual(contract.target, alias.target, `${label}: compiler expanded target changed`);
  const probe = { suffix: '<::serde_json::Value>', canonical: 'alloc::rc::Rc', callable: true,
    monomorphization: 'one fixed serde_json::Value actual callable-alias probe; no claim for every I' };
  assert.ok(context && Array.isArray(context.macroOrigins) && Array.isArray(context.externalTypeAliases), `${label}: actual compiler origin context required`);
  requireCallableCompilerIdentity(alias, probe, context.externalTypeAliases, context.macroOrigins);
  return probe;
}

// Call AFTER production external aliases have been resolved with the existing
// aliasTargetIdentity against actual artifacts/workspace/sysroot. This must also
// run for --inventory; accepting a name string is insufficient compiler proof.
export function requireCallableCompilerIdentity(alias, probe, externalAliases, macroOrigins) {
  if (!probe?.callable) return;
  const label = `${alias.name}: ${PREFIX}`;
  assert.ok(macroOrigins.includes('serde_json'), `${label}: actual serde_json dependency required`);
  const roots = externalAliases.filter(row => row.name === alias.name && row.canonical === probe.canonical);
  assert.ok(roots.length && roots.every(row => row.artifact?.kind === 'sysroot'), `${label}: actual sysroot Rc identity required`);
  const functions = externalAliases.filter(row => row.name === alias.name && row.canonical === 'core::ops::function::Fn');
  assert.ok(functions.length && functions.every(row => row.kind === 'trait' && row.artifact?.kind === 'sysroot'), `${label}: actual sysroot Fn identity required`);
}
