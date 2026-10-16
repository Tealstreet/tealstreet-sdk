import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import { reviewedInheritedConstantOutput, reviewedKucoinObserver } from './accepted-inherited-constant-output.mjs';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const reviewed = loadPolicy('constant-outputs');

// Permission belongs to exact reviewed production bodies and native
// adapters. It is never inferred from a method name or a caller's constant data.
export function reviewedConstantOutput(type, entry, index, context) {
  if (entry.constantOutput === undefined) return false;
  const label = `${type.name}: accepted-set assertion: reviewed constant output`;
  assert.equal(entry.constantOutput, true, `${label}: flag must be true`);
  if (reviewedInheritedConstantOutput(type, entry, index, context)) return true;
  const spec = reviewed[`${type.name}::${entry.api.trait}::${entry.api.name}`];
  assert.ok(spec, `${label}: unreviewed owner or method`);
  assert.ok(type.deserialize === false && type.kind === 'record' && type.name === spec.owner && type.ident === spec.ident, `${label}: actual reviewed owner required`);
  assert.equal(entry.unitInput, undefined, `${label}: constant metadata inputs are not unit input`);
  assert.deepEqual(entry.api, spec.api, `${label}: compiler API changed`);
  let packageName = 'safe-cex-num';
  if (spec.category === 'native-method') {
    packageName = reviewedNativeConstantMethod(type, entry, index, context, spec);
  } else {
    assert.equal(spec.category, undefined, `${label}: unsupported constant policy category`);
  }
  assert.ok(context.packageMetadata?.name === packageName
    && context.packageMetadata.manifest_path === fileURLToPath(new URL(`../../${packageName}/Cargo.toml`, import.meta.url)), `${label}: actual SDK compiler package required`);
  const source = type.entrypointSources?.[index];
  assert.deepEqual(source, { filename: spec.source.file, begin: spec.source.begin, end: spec.source.end }, `${label}: compiler source span changed`);
  assert.equal(typeof context.readSource, 'function', `${label}: actual source reader required`);
  assert.equal(createHash('sha256').update(context.readSource(spec.source.file)).digest('hex'), spec.source.sha256, `${label}: reviewed source changed`);
  assert.deepEqual(entry.call, spec.call, `${label}: reviewed native adapter changed`);
  assert.deepEqual(entry.cases, spec.cases, `${label}: reviewed native witnesses changed`);
  return true;
}

// Internal policy component. The production entry point above obtains spec only
// from the reviewed shard; a caller contract cannot supply a policy record.
export function reviewedNativeConstantMethod(type, entry, index, context, spec) {
  const label = `${type.name}: accepted-set assertion: reviewed constant output`;
  assert.ok(type.deserialize === false && type.kind === 'record' && type.name === spec.owner && type.ident === spec.ident, `${label}: actual reviewed owner required`);
  assert.equal(type.rawEntrypoints?.[index], true, `${label}: actual indexed raw method required`);
  assert.ok(spec.cases.length >= 2 && new Set(spec.cases.map(row => JSON.stringify(row.input))).size >= 2,
    `${label}: distinct genuine native inputs required`);
  assert.equal(typeof context.readSource, 'function', `${label}: actual source reader required`);
  if (spec.package === 'safe-cex-transport') {
    assert.ok(type.name === 'safe_cex_transport::direct::DirectTransport'
      && entry.api.trait === 'safe_cex_transport::provider::TransportProvider'
      && entry.api.name === 'ws_connection', `${label}: unreviewed native constant method`);
    assert.ok(spec.cases.every(row => typeof row.input === 'string' && row.output === null), `${label}: exact native no-connection witnesses required`);
    assert.equal(spec.typescript.file, '../packages/safe-cex/src/transport/direct.ts', `${label}: maintained direct source required`);
    const bytes = context.readSource(spec.typescript.file);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), spec.typescript.sha256, `${label}: maintained direct source changed`);
    assert.equal(bytes.toString('utf8').split('\n')[spec.typescript.line - 1].trim(), 'wsConnection: () => undefined,', `${label}: exact maintained no-connection body required`);
  } else {
    assert.ok(spec.package === 'safe-cex-venue-kucoin'
      && type.name === 'safe_cex_venue_kucoin::kucoin_exchange::KucoinSpotExchange'
      && entry.api.trait === 'safe_cex_exchange::exchange::Exchange', `${label}: unreviewed native constant method`);
    const bodies = {
      fetch_symbol_max_leverage: '  fetchSymbolMaxLeverage = async (_symbol: string): Promise<number> => {\n    return 1;\n  };',
      change_margin_type: '  changeMarginType = async (_isolated: boolean, _symbol?: string): Promise<void> => {};',
      change_position_mode: '  changePositionMode = async (_hedged: boolean, _symbol?: string): Promise<void> => {};',
    };
    const body = bodies[entry.api.name];
    assert.equal(typeof body, 'string', `${label}: unreviewed Spot constant override`);
    const source = spec.typescript.override;
    assert.equal(source.file, '../packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts', `${label}: maintained Spot source required`);
    const bytes = context.readSource(source.file);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), source.sha256, `${label}: maintained Spot source changed`);
    assert.equal(source.excerpt, body, `${label}: exact reviewed Spot override required`);
    assert.equal(bytes.toString('utf8').split('\n').slice(source.begin[0] - 1, source.end[0]).join('\n'), body, `${label}: maintained Spot override moved`);
    assert.equal(createHash('sha256').update(context.readSource(spec.fixture.file)).digest('hex'), spec.fixture.sha256, `${label}: reviewed native fixture changed`);
    reviewedKucoinObserver(spec, context, label, 'safe-cex-parity/accepted-set/kucoin-constant-fixture.rs');
    const native = entry.api.name === 'fetch_symbol_max_leverage' ? { kind: 'number', bits: '3ff0000000000000' } : { kind: 'unit' };
    for (const row of spec.cases) {
      assert.deepEqual(row.output.native, native, `${label}: exact native Spot constant required`);
      assert.ok(row.output.http === 0 && row.output.notifications === 0 && row.output.events === 0
        && ['state_unchanged', 'options_unchanged', 'reports_unchanged', 'sockets_unchanged', 'now_unchanged'].every(key => row.output[key] === true), `${label}: native unchanged state and zero I/O required`);
    }
  }
  return spec.package;
}
