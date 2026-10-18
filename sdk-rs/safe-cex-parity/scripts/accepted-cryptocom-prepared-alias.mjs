import assert from 'node:assert/strict';
import {loadPolicy} from './accepted-policy-files.mjs';

const owner = 'safe_cex_venue_cryptocom::native_bindings::Prepared';
const specs = loadPolicy('generic-aliases');
const fields = ['canonical', 'expectedType', 'file', 'generics', 'rawTarget', 'suffix', 'target', 'unsized'];
const callerFields = ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical'];

export function cryptocomPreparedAliasProbe(alias, contract, context, registrations = specs) {
  if (alias.name !== owner || !registrations[owner]) return undefined;
  const spec = registrations[owner];
  const label = `${owner}: accepted-set assertion: reviewed prepared-call tuple`;
  assert.deepEqual(Object.keys(spec).sort(), fields, `${label}: unsupported registration fields`);
  assert.equal(alias.ident, 'Prepared', `${label}: declaration identity changed`);
  assert.equal(spec.file, 'safe-cex-venue-cryptocom/src/native_bindings.rs', `${label}: reviewed source identity required`);
  assert.equal(alias.span?.filename, spec.file, `${label}: compiler source identity changed`);
  const generics = {params:[{name:'V',kind:{type:{bounds:[],default:null,is_synthetic:false}}}],where_predicates:[]};
  const tuple = {tuple:[{generic:'V'},{generic:'V'}]};
  assert.deepEqual(alias.generics, generics, `${label}: one unbounded source parameter required`);
  for (const key of ['generics', 'rawTarget', 'target']) {
    assert.deepEqual(alias[key], spec[key], `${label}: compiler ${key} changed`);
  }
  assert.deepEqual(alias.rawTarget, tuple, `${label}: source must be its actual prepared pair`);
  assert.deepEqual(alias.target, tuple, `${label}: expanded target must preserve pair order`);
  assert.equal(spec.canonical, 'tuple', `${label}: only the reviewed tuple role is supported`);
  assert.equal(spec.unsized, false, `${label}: prepared pair is sized`);
  assert.match(spec.suffix, /^<[^<>,]+>$/, `${label}: one fixed instantiation required`);
  assert.ok(typeof spec.expectedType === 'string' && spec.expectedType.trim(), `${label}: independent expected Rust type required`);
  for (const key of callerFields) assert.equal(contract?.[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler context required`);
  return {suffix:spec.suffix,canonical:'tuple',expectedType:spec.expectedType,
    probeKind:'reviewed-cryptocom-prepared',
    monomorphization:`one registered ${owner}${spec.suffix} prepared-pair instantiation; no claim for another V or tuple alias`};
}
