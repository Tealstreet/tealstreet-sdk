import assert from 'node:assert/strict';
import { test } from 'node:test';
import { flatPositionPnl, normalizeAccount } from './comparison-policy.mjs';
import { compareSnapshots } from './snapshot-compare.mjs';

const account = () => ({ balance: {}, orders: [], positions: [], settings: [], fills: [] });
const snapshot = (value) => ({ markets: [], tickers: [], books: {}, account: value });
const normalize = (value, impl = 'ts') => normalizeAccount(value, { rust: 'binance' }, impl);

test('flat position realized PnL survives placeholder filtering, including zero and missing evidence', () => {
  const value = account();
  value.positions = [{ symbol: 'BTC', side: 'long', contracts: 0, realizedPnl: 5 }];
  const original = normalize(value);
  assert.equal(original.positions.length, 0);
  assert.equal(original.flatPositionPnl[0].realizedPnl, 5);
  for (const realizedPnl of [999, 0, null, undefined]) {
    value.positions[0].realizedPnl = realizedPnl;
    assert.match(compareSnapshots(snapshot(original), snapshot(normalize(value))).join('\n'), /flatPositionPnl/);
  }
  // TS capture extracts financial data before removing its placeholder rows.
  value.positions[0].realizedPnl = 5;
  const captured = { ...value, flatPositionPnl: flatPositionPnl(value.positions), positions: [] };
  assert.deepEqual(normalize(captured).flatPositionPnl, original.flatPositionPnl);
  const native = normalize(value, 'rust');
  assert.deepEqual(native.flatPositionPnl, original.flatPositionPnl);
});

test('unavailable flat-position PnL cannot manufacture a row from a native NaN sentinel', () => {
  for (const realizedPnl of [NaN, Infinity, -Infinity, undefined, null, '', ' ', 'unavailable', true, false, {}, []]) {
    const positions = [{ symbol: 'ETHUSD_PERP', side: 'long', positionMode: 'oneway', contracts: 0, realizedPnl }];
    const before = structuredClone(positions);
    assert.deepEqual(flatPositionPnl(positions), [], String(realizedPnl));
    assert.deepEqual(positions, before);
  }
});

test('TS absence and the native unavailable sentinel normalize to the same flat-position evidence', () => {
  const ts = account();
  ts.positions = [{ symbol: 'ETHUSD_PERP', side: 'long', positionMode: 'oneway', contracts: 0 }];
  const rust = structuredClone(ts);
  rust.positions[0].realizedPnl = new Float64Array([NaN])[0];
  const expected = normalize(ts);
  const actual = normalize(rust, 'rust');
  assert.deepEqual(actual.flatPositionPnl, expected.flatPositionPnl);
  assert.deepEqual(compareSnapshots(snapshot(expected), snapshot(actual)), []);
});

test('finite flat-position PnL retains signed zero, losses and numeric-string evidence', () => {
  for (const realizedPnl of [0, -0, -5, 7.25, '0', '-5', '7.25', '1e3']) {
    const positions = [{ symbol: 'ETHUSD_PERP', side: 'long', positionMode: 'oneway', contracts: 0, realizedPnl }];
    const before = structuredClone(positions);
    assert.deepEqual(flatPositionPnl(positions), [{
      symbol: 'ETHUSD_PERP', side: 'long', positionMode: 'oneway', realizedPnl: Number(realizedPnl),
    }]);
    assert.deepEqual(positions, before);
  }
});

test('non-flat PnL stays in the ordinary position view and real losses remain mismatches', () => {
  const value = account();
  value.positions = [{ symbol: 'ETHUSD_PERP', side: 'long', contracts: 2, realizedPnl: -5 }];
  assert.deepEqual(flatPositionPnl(value.positions), []);
  assert.equal(normalize(value, 'rust').positions[0].realizedPnl, -5);
  value.positions[0].contracts = 0;
  const expected = normalize(value);
  for (const realizedPnl of [0, 5, NaN, undefined, null]) {
    const different = structuredClone(value);
    different.positions[0].realizedPnl = realizedPnl;
    assert.match(compareSnapshots(snapshot(expected), snapshot(normalize(different, 'rust'))).join('\n'), /flatPositionPnl/);
  }
});

test('both bracket trigger sources compare independently', () => {
  const value = account();
  value.orders = [{ id: 'bracket', symbol: 'BTC', type: 'limit', tpTriggerType: 'mark_price', slTriggerType: 'last_price' }];
  const original = normalize(value);
  value.orders[0].slTriggerType = 'index_price';
  assert.match(compareSnapshots(snapshot(original), snapshot(normalize(value))).join('\n'), /stopLossTrigger/);
  value.orders[0].slTriggerType = 'last_price';
  value.orders[0].tpTriggerType = 'index_price';
  assert.match(compareSnapshots(snapshot(original), snapshot(normalize(value))).join('\n'), /takeProfitTrigger/);
});

test('a standalone protective order uses its actual generic trigger source for its own leg only', () => {
  for (const [type, leg, other] of [['take_profit_market', 'takeProfitTrigger', 'stopLossTrigger'], ['stop_market', 'stopLossTrigger', 'takeProfitTrigger']]) {
    const value = account();
    value.orders = [{ id: 'protective', type, triggerType: 'mark_price' }];
    assert.equal(normalize(value, 'rust').orders[0][leg], 'mark_price');
    assert.equal(normalize(value, 'rust').orders[0][other], null);
  }
});
