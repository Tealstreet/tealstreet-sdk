import assert from 'node:assert/strict';

const name = 'safe_cex_venue_woo::v3_ws_public::WooV3WsBookLevel';
const string = 'alloc::string::String';
const target = { array: { type: { resolved_path: { path: string, args: null } }, len: '2' } };

export function wooBookLevelArrayProbe(alias, contract, context) {
  if (alias.name !== name) return undefined;
  const label = `${name}: accepted-set assertion: reviewed WOO book level array`;
  assert.equal(alias.ident, 'WooV3WsBookLevel', `${label}: identifier changed`);
  assert.equal(alias.span?.filename, 'safe-cex-venue-woo/src/v3_ws_public.rs', `${label}: source identity changed`);
  assert.deepEqual(alias.generics, { params: [], where_predicates: [] }, `${label}: concrete nongeneric alias required`);
  assert.deepEqual(alias.rawTarget, target, `${label}: ordered raw slots changed`);
  assert.deepEqual(alias.target, target, `${label}: ordered expanded slots changed`);
  for (const key of ['generics', 'rawTarget', 'target']) assert.deepEqual(contract?.[key], alias[key], `${label}: contract compiler ${key} changed`);
  assert.ok(contract?.category === 'decoder' && contract.shape === 'union' && contract.evidence?.trim(), `${label}: explicit array decoder contract required`);
  for (const key of ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical', 'expectedType', 'suffix', 'probeKind', 'unsized']) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler context required`);
  const roots = context.externalTypeAliases.filter(row => row.name === name).map(row => ({ canonical: row.canonical, kind: row.kind, artifactKind: row.artifact?.kind }));
  assert.deepEqual(roots, [{ canonical: string, kind: 'struct', artifactKind: 'sysroot' }], `${label}: actual compiler String root identity required`);
  return {
    suffix: '', canonical: 'array', expectedType: '[::std::string::String;2]', probeKind: 'reviewed-woo-book-level-array',
    monomorphization: 'the actual concrete two-slot WooV3WsBookLevel; no generic or other array permission',
  };
}
