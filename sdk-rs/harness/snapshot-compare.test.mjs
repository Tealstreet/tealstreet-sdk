import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compareCheckpoints, compareSnapshots, duplicateSymbols } from './snapshot-compare.mjs';

const snapshot = () => ({
  markets: [{ symbol: 'BTC' }], tickers: [{ symbol: 'BTC', last: 100 }], books: {},
  account: {
    orders: [{ id: '1', symbol: 'BTC', clientOrderId: 'caller', takeProfit: 110, stopLoss: 90, conditionalOrderPrice: 99, parentId: 'parent' }],
    positions: [{ symbol: 'BTC', realizedPnl: 7 }],
    fills: [{ id: '1', price: 100, fee: 0.1, timestamp: 123, executionId: 'exec' }],
  },
});

test('snapshot comparison detects each widened economic and identity field', () => {
  for (const [section, fields] of [
    ['orders', ['clientOrderId', 'takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId']],
    ['positions', ['realizedPnl']],
    ['fills', ['price', 'fee', 'timestamp', 'executionId']],
  ]) for (const field of fields) {
    const left = snapshot();
    const right = snapshot();
    right.account[section][0][field] = 'incorrect';
    assert.ok(compareSnapshots(left, right).length, `${section}.${field}`);
  }
});

test('duplicate symbols fail even when duplicates and both answers agree', () => {
  for (const field of ['markets', 'tickers']) {
    const value = snapshot();
    value[field].push({ ...value[field][0] });
    assert.match(compareSnapshots(value, structuredClone(value)).join('\n'), /duplicate symbol BTC/);
  }
  assert.deepEqual(duplicateSymbols([{ symbol: 'BTC' }, { symbol: 'ETH' }], 'rows'), []);
  const filtered = { markets: [], tickers: [], books: {}, duplicateSymbols: ['snapshot: duplicate markets symbol "SPOT" at rows 0 and 1'] };
  assert.match(compareSnapshots(filtered, structuredClone(filtered)).join('\n'), /duplicate markets symbol/);
});

test('intermediate snapshots detect transient defects despite final convergence', () => {
  const checkpoints = [0, 1, 2].map((ordinal) => ({ ordinal, recordIndex: ordinal, t: ordinal * 250, ...snapshot() }));
  const candidate = structuredClone(checkpoints);
  candidate[1].tickers[0].last = 1;
  assert.match(compareCheckpoints(checkpoints, candidate).join('\n'), /checkpoint 1 tickers/);
  assert.deepEqual(candidate[2], checkpoints[2]);
  candidate[1] = structuredClone(checkpoints[1]);
  candidate[1].t++;
  assert.match(compareCheckpoints(checkpoints, candidate).join('\n'), /checkpoint 1 t:/);
  assert.deepEqual(compareCheckpoints(checkpoints, structuredClone(checkpoints)), []);
  assert.match(compareCheckpoints([], []).join('\n'), /missing/);
  assert.match(compareCheckpoints([snapshot()], [snapshot()]).join('\n'), /invalid tape boundary/);
});
