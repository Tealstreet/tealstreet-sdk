import assert from 'node:assert/strict';
import { test } from 'node:test';
import { normalizeAccount } from './comparison-policy.mjs';
import { snapshotCoverageErrors, compareSnapshots } from './snapshot-compare.mjs';

const source = () => ({ hedged: false, balance: null, orders: [{}], positions: [{}], settings: [{}], fills: [{}] });
const normalize = (account, extra = {}) => normalizeAccount(account, { rust: 'fixture', ...extra }, 'ts');
const snapshot = (account) => ({ markets: [], tickers: [], books: {}, account });

test('normalized missing row values survive JSON as explicit unavailable evidence', () => {
  for (const extra of [{}, { independentLeverage: true }, { inverseNativeAmounts: true }, { coinNativeAmounts: true }]) {
    const value = normalize(source(), extra);
    const serialized = JSON.parse(JSON.stringify(value));
    assert.deepEqual(serialized, value);
    assert.deepEqual(snapshotCoverageErrors(snapshot(serialized), 'private', { isPrivate: true }), []);
    for (const section of ['orders', 'positions', 'settings', 'fills'])
      assert.ok(Object.values(value[section][0]).every((item) => item === null || typeof item === 'boolean'), section);
  }
});

test('normalization preserves reported base numeric strings and enums and does not substitute zero', () => {
  const account = source();
  account.orders[0] = { id: 'order', symbol: 'BTC', type: 'limit', price: '100.125', amount: '2', filled: '1', remaining: '1', side: 'buy', status: 'open', timeInForce: 'IOC' };
  account.positions[0] = { symbol: 'BTC', side: 'long', contracts: '2', notional: '200.25', leverage: '10', entryPrice: '100.125', unrealizedPnl: '5', liquidationPrice: '50', marginType: 'cross' };
  account.settings[0] = { symbol: 'BTC', marginType: 'isolated', leverage: '10' };
  account.fills[0] = { id: 'execution', symbol: 'BTC', side: 'buy', amount: '1' };
  const normalized = normalize(account);
  for (const section of ['orders', 'positions', 'settings', 'fills']) for (const [field, value] of Object.entries(account[section][0]))
    assert.equal(normalized[section][0][field], value, `${section}.${field}`);
  assert.equal(normalized.fills[0].price, null);
  const zero = structuredClone(normalized); zero.fills[0].price = 0;
  assert.ok(compareSnapshots(snapshot(normalized), snapshot(zero)).some((error) => error.includes('fills[0].price')));
  const missingOrderPrice = source(); missingOrderPrice.orders[0] = { symbol: 'BTC' };
  const unavailable = normalize(missingOrderPrice);
  assert.equal(unavailable.orders[0].price, null);
  const zeroOrder = structuredClone(unavailable); zeroOrder.orders[0].price = 0;
  assert.ok(compareSnapshots(snapshot(unavailable), snapshot(zeroOrder)).some((error) => error.includes('orders[0].price')));
});
