import assert from 'node:assert/strict';
import { cryptocomPreparedAliasProbe } from './accepted-cryptocom-prepared-alias.mjs';
import { identityAliasProbe } from './accepted-identity-alias.mjs';
import { reviewedGenericAliasProbe } from './accepted-reviewed-aliases.mjs';
import { reviewedFunctionPointerAliasProbe } from './accepted-function-pointer-alias.mjs';
import { reviewedNapiAliasProbe } from './accepted-napi-alias.mjs';
import { weexKlineTupleProbe } from './accepted-weex-kline-alias.mjs';
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
function boundedAst(node, parameters) {
  if (node === null || typeof node !== 'object') return;
  if (Array.isArray(node)) { for (const value of node) boundedAst(value, parameters); return; }
  // This initial family does not prove binder substitution or borrowed types.
  if (Object.hasOwn(node, 'generic')) assert.ok(parameters.includes(node.generic), `${PREFIX}: unrelated generic`);
  if (node.dyn_trait) assert.equal(node.dyn_trait.traits?.length, 1, `${PREFIX}: multiple dyn bounds unsupported`);
  if (Object.hasOwn(node, 'generic_params')) assert.deepEqual(node.generic_params, [], `${PREFIX}: higher-ranked parameters unsupported`);
  if (Object.hasOwn(node, 'lifetime')) assert.equal(node.lifetime, null, `${PREFIX}: explicit lifetimes unsupported`);
  for (const value of Object.values(node)) boundedAst(value, parameters);
}
export function genericAliasProbe(alias, contract, context) {
  const prepared = cryptocomPreparedAliasProbe(alias, contract, context);
  if (prepared) return prepared;
  const napi = reviewedNapiAliasProbe(alias, contract, context);
  if (napi) return napi;
  const hosted = hostedSignerTupleProbe(alias, contract, context);
  if (hosted) return hosted;
  const weex = weexKlineTupleProbe(alias, contract, context);
  if (weex) return weex;
  const pointer = reviewedFunctionPointerAliasProbe(alias, contract, context);
  if (pointer) return pointer;
  if (!(alias.generics?.params ?? []).length) return undefined;
  const reviewed = reviewedGenericAliasProbe(alias, contract, context);
  if (reviewed) return reviewed;
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
  assert.ok(parameters.length === 1 || parameters.length === 2, `${label}: one or two unbounded type parameters required`);
  const names = parameters.map(parameter => parameter.name);
  assert.ok(names.every(name => name && !name.startsWith("'")) && new Set(names).size === names.length, `${label}: distinct named type parameters required`);
  assert.deepEqual(alias.generics, { params: names.map(name => ({ name, kind: { type: { bounds: [], default: null, is_synthetic: false } } })), where_predicates: [] }, `${label}: bounds/defaults/lifetimes/const/where clauses unsupported`);
  boundedAst(alias.rawTarget, names);
  boundedAst(alias.target, names);
  assert.equal(contract?.category, 'non-decoder', `${label}: only explicit non-decoder classification supported`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: bounded classification evidence required`);
  assert.equal(contract.instantiation, undefined, `${label}: caller cannot redirect the production alias`);
  assert.deepEqual(contract.generics, alias.generics, `${label}: compiler generics changed`);
  assert.deepEqual(contract.rawTarget, alias.rawTarget, `${label}: compiler source target changed`);
  assert.deepEqual(contract.target, alias.target, `${label}: compiler expanded target changed`);
  const probe = { suffix: `<${names.map(() => '::serde_json::Value').join(',')}>`, canonical: 'alloc::rc::Rc', callable: true,
    monomorphization: `one fixed ${names.map(() => 'serde_json::Value').join('/')} actual callable-alias probe; no claim for every I` };
  assert.ok(context && Array.isArray(context.macroOrigins) && Array.isArray(context.externalTypeAliases), `${label}: actual compiler origin context required`);
  requireCallableCompilerIdentity(alias, probe, context.externalTypeAliases, context.macroOrigins);
  return probe;
}

// One concrete tuple alias. Its expected native type belongs to this probe,
// never the contract. The ordinary alias trait probes still execute on it.
function hostedSignerTupleProbe(alias, contract, context) {
  if (alias.name !== 'safe_cex_exchange::js_runtime::base_exchange::HostedSigner') return undefined;
  const label = `${alias.name}: accepted-set assertion: hosted signer tuple`;
  const path = name => ({ resolved_path: { path: name, args: null } });
  const applied = (name, type) => ({ resolved_path: { path: name, args: { angle_bracketed: { args: [{ type }], constraints: [] } } } });
  const target = { tuple: [applied('alloc::rc::Rc', { dyn_trait: { traits: [{ trait: { path: 'safe_cex_exchange::runtime::RemoteSigner', args: null }, generic_params: [] }], lifetime: null } }), applied('core::option::Option', path('alloc::string::String'))] };
  assert.equal(alias.ident, 'HostedSigner', `${label}: identifier changed`);
  assert.equal(alias.span?.filename, 'safe-cex-exchange/src/js_runtime/base_exchange.rs', `${label}: source identity changed`);
  assert.deepEqual(alias.generics, { params: [], where_predicates: [] }, `${label}: generic parameters changed`);
  assert.deepEqual(alias.rawTarget, target, `${label}: source target changed`);
  assert.deepEqual(alias.target, target, `${label}: expanded target changed`);
  assert.equal(contract?.category, 'non-decoder', `${label}: explicit capability classification required`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: specific classification required`);
  for (const key of ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical']) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);
  for (const key of ['generics', 'rawTarget', 'target']) assert.deepEqual(contract[key], alias[key], `${label}: contract compiler ${key} changed`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler context required`);
  for (const [canonical, kind] of [['alloc::rc::Rc', 'struct'], ['core::option::Option', 'enum'], ['alloc::string::String', 'struct']]) {
    const rows = context.externalTypeAliases.filter(row => row.name === alias.name && row.canonical === canonical);
    assert.ok(rows.length === 1 && rows[0].kind === kind && rows[0].artifact?.kind === 'sysroot', `${label}: actual sysroot ${canonical} identity required`);
  }
  return { suffix: '', canonical: 'tuple', expectedType: '(::std::rc::Rc<dyn crate::runtime::RemoteSigner>,::core::option::Option<::std::string::String>)', probeKind: 'reviewed-hosted-signer-tuple', monomorphization: 'the actual concrete HostedSigner tuple; no signer execution, account parsing or generic tuple permission' };
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
