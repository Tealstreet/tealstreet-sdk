import assert from 'node:assert/strict';

// One concrete protocol tuple. The policy owns both the ordered compiler AST
// and the invariant native type; callers cannot select another instantiation.
const name = 'safe_cex_venue_weex::response_types::WeexKlineRow';
const number = 'safe_cex_venue_weex::response_types::WeexNumberOrString';
const string = 'alloc::string::String';
const slots = [number, string, string, string, string, string, number, string, number, string, string];
const target = { tuple: slots.map(path => ({ resolved_path: { path, args: null } })) };
const expectedType = '(crate::response_types::WeexNumberOrString,::std::string::String,::std::string::String,::std::string::String,::std::string::String,::std::string::String,crate::response_types::WeexNumberOrString,::std::string::String,crate::response_types::WeexNumberOrString,::std::string::String,::std::string::String)';

export function weexKlineTupleProbe(alias, contract, context) {
  if (alias.name !== name) return undefined;
  const label = `${name}: accepted-set assertion: reviewed Weex kline tuple`;
  assert.equal(alias.ident, 'WeexKlineRow', `${label}: identifier changed`);
  assert.equal(alias.span?.filename, 'safe-cex-venue-weex/src/response_types.rs', `${label}: source identity changed`);
  assert.deepEqual(alias.generics, { params: [], where_predicates: [] }, `${label}: concrete nongeneric alias required`);
  assert.deepEqual(alias.rawTarget, target, `${label}: ordered raw slots changed`);
  assert.deepEqual(alias.target, target, `${label}: ordered expanded slots changed`);
  for (const key of ['generics', 'rawTarget', 'target']) assert.deepEqual(contract?.[key], alias[key], `${label}: contract compiler ${key} changed`);
  assert.ok(contract?.category === 'decoder' && contract.shape === 'union' && contract.evidence?.trim(), `${label}: explicit tuple decoder contract required`);
  for (const key of ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical', 'expectedType', 'suffix', 'probeKind', 'unsized']) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler context required`);
  const roots = context.externalTypeAliases.filter(row => row.name === name).map(row => ({ canonical: row.canonical, kind: row.kind, artifactKind: row.artifact?.kind }));
  // The three NumberOrString slots are local compiler owners. Each of the eight
  // repeated String slots must independently resolve to authenticated sysroot.
  assert.deepEqual(roots, Array.from({ length: 8 }, () => ({ canonical: string, kind: 'struct', artifactKind: 'sysroot' })), `${label}: every repeated compiler root identity required`);
  return {
    suffix: '', canonical: 'tuple', expectedType, probeKind: 'reviewed-weex-kline-tuple',
    monomorphization: 'the actual concrete eleven-slot WeexKlineRow; no generic or other tuple permission',
  };
}
