import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { loadPolicy } from './accepted-policy-files.mjs';
import { hyperliquidProjectionCompilerIdentity } from './accepted-hyperliquid-projection.mjs';
import { reviewedGenericAliasProbe } from './accepted-reviewed-aliases.mjs';
import { aliasContract } from './accepted-aliases.mjs';

const projectionNames = [
  'safe_cex_venue_hyperliquid::private_class::PrivateValue',
  'safe_cex_venue_hyperliquid::private_class::PrivateError',
  'safe_cex_venue_hyperliquid::public_class::PublicValue',
  'safe_cex_venue_hyperliquid::public_class::PublicError',
  'safe_cex_venue_hyperliquid::core_exchange_class::ExchangeValue',
  'safe_cex_venue_hyperliquid::core_exchange_class::ExchangeError',
];
const specs = Object.fromEntries(Object.entries(loadPolicy('generic-aliases')).filter(([name]) => projectionNames.includes(name)));
assert.deepEqual(Object.keys(specs).sort(), [...projectionNames].sort(), 'all six reviewed associated projections remain selected');
const workspace = fileURLToPath(new URL('../../', import.meta.url));

// Controlled compiler-role fixture. Actual rustdoc/artifact/native execution is
// covered separately; no source or runtime observation is inferred from this.
export function projectionFixture(name = Object.keys(specs)[0]) {
  const spec = specs[name], member = spec.rawTarget.qualified_path.name;
  const root = (id, canonical, kind, file, inner) => ({ id, crate_id: 0, name: canonical.split('::').at(-1), span: { filename: file }, inner: { [kind]: { generics: { params: [], where_predicates: [] }, ...inner } } });
  const prefix = 'safe_cex_venue_hyperliquid';
  const doc = { paths: {}, index: {
    1: root(1, `${prefix}::ws_ping_lifecycle::WsPingHost`, 'trait', 'safe-cex-venue-hyperliquid/src/ws_ping_lifecycle.rs', { items: [13, 14], implementations: [4] }),
    2: root(2, `${prefix}::realm::Realm`, 'struct', 'safe-cex-venue-hyperliquid/src/realm/mod.rs', { impls: [4] }),
    3: root(3, `${prefix}::realm::value::Value`, 'enum', 'safe-cex-venue-hyperliquid/src/realm/value.rs', {}),
    4: { id: 4, crate_id: 0, span: { filename: 'safe-cex-venue-hyperliquid/src/realm/host_core.rs' }, inner: { impl: {
      trait: { id: 1, args: null }, for: { resolved_path: { id: 2, args: null } }, items: [5, 6], generics: { params: [], where_predicates: [] },
      is_unsafe: false, is_negative: false, is_synthetic: false, blanket_impl: null,
    } } },
  } };
  for (const [id, canonical, kind] of [[1, `${prefix}::ws_ping_lifecycle::WsPingHost`, 'trait'], [2, `${prefix}::realm::Realm`, 'struct'], [3, `${prefix}::realm::value::Value`, 'enum']]) doc.paths[id] = { crate_id: 0, path: canonical.split('::'), kind };
  for (const [id, label] of [[5, 'Value'], [6, 'Error'], [13, 'Value'], [14, 'Error']]) doc.index[id] = {
    id, name: label, crate_id: 0, span: { filename: 'safe-cex-venue-hyperliquid/src/realm/host_core.rs' },
    inner: { assoc_type: { generics: { params: [], where_predicates: [] }, bounds: [], type: { resolved_path: { id: 3, args: null } } } },
  };
  const item = { id: 7, inner: { type_alias: { type: { qualified_path: { trait: { id: 1 } } } } } };
  const paths = new Map(Object.entries(doc.paths).map(([id, value]) => [id, value.path])); paths.set('7', name.split('::'));
  const identity = hyperliquidProjectionCompilerIdentity(doc, item, paths);
  assert.equal(identity.binding.name, member);
  const alias = { name, ident: name.split('::').at(-1), span: { filename: spec.file }, generics: spec.generics, rawTarget: spec.rawTarget, target: spec.target, projectionIdentity: identity };
  const contract = { category: 'non-decoder', reason: 'One Realm value/error capability; decoder and conversion absence is a separate native obligation.', evidence: 'Controlled compiler-role fixture.', target: spec.target, rawTarget: spec.rawTarget, generics: spec.generics };
  const context = { workspace, packageMetadata: { name: 'safe-cex-venue-hyperliquid', manifest_path: path.join(workspace, 'safe-cex-venue-hyperliquid/Cargo.toml') }, macroOrigins: ['serde_json'], externalTypeAliases: [] };
  return { doc, item, paths, alias, contract, context, spec };
}

test('six associated projections retain exact compiler roots implementation and fixed host', () => {
  assert.equal(Object.keys(specs).length, 6);
  for (const name of Object.keys(specs)) {
    const f = projectionFixture(name);
    const contract = aliasContract(f.alias, { [name]: f.contract }, f.context);
    assert.equal(contract.aliasProbeKind, 'reviewed-fixed');
    assert.equal(contract.aliasProbeExpected, 'crate::realm::value::Value');
    assert.equal(contract.aliasProbeSuffix, '<crate::realm::Realm>');
    for (const change of [
      x => { x.doc.index[1].crate_id = 1; },
      x => { x.doc.paths[3].kind = 'struct'; },
      x => { x.doc.index[1].inner.trait.generics.params = [{ name: 'T' }]; },
      x => { x.doc.index[2].inner.struct.generics.params = [{ name: 'T' }]; },
      x => { x.doc.index[3].inner.enum.generics.params = [{ name: 'T' }]; },
      x => { x.doc.index[2].inner.struct.impls = []; },
      x => { x.doc.index[1].inner.trait.implementations = []; },
      x => { x.doc.index[4].inner.impl.generics.params = [{ name: 'T' }]; },
      x => { x.doc.index[4].inner.impl.is_negative = true; },
      x => { x.doc.index[4].inner.impl.trait.args = {}; },
      x => { x.doc.index[5].inner.assoc_type.type.resolved_path.args = {}; x.doc.index[6].inner.assoc_type.type.resolved_path.args = {}; },
      x => { x.doc.index[4].span.filename = 'another.rs'; },
      x => { x.doc.index[40] = structuredClone(x.doc.index[4]); x.doc.index[40].id = 40; },
    ]) {
      const changed = structuredClone(f); change(changed);
      assert.throws(() => hyperliquidProjectionCompilerIdentity(changed.doc, changed.item, changed.paths), /associated projection compiler identity/);
    }
    for (const key of ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical', 'projectionIdentity']) assert.throws(() => aliasContract(f.alias, { [name]: { ...f.contract, [key]: 'redirect' } }, f.context), /caller cannot supply|cannot be supplied/);
    assert.throws(() => reviewedGenericAliasProbe({ ...f.alias, projectionIdentity: undefined }, f.contract, f.context), /actual compiler associated identity/);
    assert.throws(() => reviewedGenericAliasProbe(f.alias, f.contract, { ...f.context, macroOrigins: [] }), /compiler dependency context/);
    assert.throws(() => reviewedGenericAliasProbe(f.alias, f.contract, { ...f.context, packageMetadata: { ...f.context.packageMetadata, name: 'other' } }), /actual selected package/);
    assert.throws(() => aliasContract({ ...f.alias, name: 'unreviewed::Associated' }, { 'unreviewed::Associated': f.contract }, f.context), /only direct identity parameters|source must directly name/);
  }
});

test('associated projection compiler provenance and fixed policy guards reject executed mutations', () => {
  const base = projectionFixture(), url = new URL('./accepted-hyperliquid-projection.mjs', import.meta.url).href;
  const policy = new URL('./accepted-reviewed-aliases.mjs', import.meta.url).href;
  const rows = [
    ['projection same-crate compiler root', "    assert.ok(value?.crate_id === 0 && canonical?.crate_id === 0 && canonical.kind === kind\n      && value.inner[kind] && paths.get(String(id))?.join('::') === canonical.path.join('::'), `${label}: actual same-crate ${kind} root required`);", f => { f.doc.index[1].crate_id = 1; }, 'compiler', /same-crate/],
    ['projection compiler associated identity', '  assert.deepEqual(alias.projectionIdentity, expectedIdentity(declaration.member), `${label}: actual compiler associated identity required`);', f => { f.alias.projectionIdentity.binding.name = 'Other'; }, 'policy', /actual compiler associated identity/],
    ['projection exact raw target', '  assert.deepEqual(alias.rawTarget, target, `${label}: exact raw associated projection required`);', f => { f.alias.rawTarget.qualified_path.self_type.generic = 'Other'; f.spec.rawTarget = f.alias.rawTarget; }, 'policy', /exact raw associated projection/],
    ['projection fixed Realm host', "  assert.equal(spec.suffix, '<crate::realm::Realm>', `${label}: only the fixed Realm host is reviewed`);", f => { f.spec.suffix = '<OtherHost>'; }, 'policy', /fixed Realm host/],
    ['projection selected workspace source', "  assert.equal(fs.realpathSync(context.packageMetadata.manifest_path), fs.realpathSync(path.join(context.workspace, packageName, 'Cargo.toml')), `${label}: selected workspace source required`);", f => { f.context.packageMetadata.manifest_path = path.join(workspace, 'safe-cex-num/Cargo.toml'); }, 'policy', /selected workspace source/],
  ];
  for (const [label, anchor, change, mode, refusal] of rows) {
    const f = structuredClone(base); change(f); f.paths = [...f.paths];
    const run = mutant => spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import{registerHooks}from'node:module';const f=${JSON.stringify(f)};let changed=0;registerHooks({load(url,ctx,next){const r=next(url,ctx);if(!${mutant}||url!==${JSON.stringify(url)})return r;const s=String(r.source),anchor=${JSON.stringify(anchor)};assert.equal(s.split(anchor).length,2);changed++;return{...r,source:s.replace(anchor,'')};}});const{hyperliquidProjectionCompilerIdentity}=await import(${JSON.stringify(url)});const{reviewedGenericAliasProbe}=await import(${JSON.stringify(policy)});assert.throws(()=>${mode === 'compiler' ? 'hyperliquidProjectionCompilerIdentity(f.doc,f.item,new Map(f.paths))' : 'reviewedGenericAliasProbe(f.alias,f.contract,f.context,{[f.alias.name]:f.spec})'},${refusal},${JSON.stringify(label + ' must refuse')});assert.equal(changed,${mutant ? 1 : 0});`], { encoding: 'utf8' });
    let result = run(false); assert.equal(result.status, 0, result.stderr);
    result = run(true); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, new RegExp(label + ' must refuse'));
    console.log(`${label}: named assertion; process exit 1`);
    result = run(false); assert.equal(result.status, 0, result.stderr);
  }
});
