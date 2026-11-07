import assert from 'node:assert/strict';
import { test } from 'node:test';
import { currencyBalances, lookupResult } from './account-shapes.mjs';

test('currency projection retains every financial amount and currency', () => {
  const values = { BTC: { free: 0.3, used: 0.1, total: 0.4, upnl: -1, info: { balance: '0.4' } },
    USDT: { free: 0, used: 10, total: 10, upnl: 0 } };
  assert.deepEqual(currencyBalances(values), { BTC: { free: 0.3, used: 0.1, total: 0.4, upnl: -1 }, USDT: values.USDT });
  assert.equal(values.BTC.info.balance, '0.4');
  assert.notDeepEqual(currencyBalances(values), currencyBalances({ ...values, BTC: { ...values.BTC, total: 0.5 } }));
});

test('lookup projection compares financial and native client identity across wire metadata placement', () => {
  const command = { op: 'findOrderByClientOrderId' };
  const order = { id: '1', symbol: 'BTC-USDT', status: 'open', type: 'limit', side: 'buy',
    price: 10, amount: 2, filled: 0.5, remaining: 1.5, timeInForce: 'GTC' };
  const ts = { ok: { found: true, order: { ...order, info: { clientOid: 'caller' }, lastUpdate: 1 } } };
  const rust = { ok: { found: true, order: { ...order, clientOrderId: 'caller', lastUpdate: 2 } } };
  assert.deepEqual(lookupResult(ts, command), lookupResult(rust, command));
  assert.deepEqual(lookupResult({ ok: { found: true, order: { ...order, info: { c: 'caller' } } } }, command), lookupResult(rust, command));
  for (const [field, value] of Object.entries({ filled: 0.6, remaining: 1.4, amount: 3, price: 11,
    id: '2', side: 'sell', status: 'closed', clientOrderId: 'wrong' })) {
    assert.notDeepEqual(lookupResult(ts, command), lookupResult({ ok: { found: true, order: { ...rust.ok.order, [field]: value } } }, command));
  }
  assert.equal(lookupResult(ts, { op: 'unrelated' }), ts);
  const miss = { ok: { found: false, definitive: false } };
  assert.equal(lookupResult(miss, command), miss);
  assert.notDeepEqual(lookupResult(miss, command), lookupResult({ ok: { found: false, definitive: true } }, command));
});
