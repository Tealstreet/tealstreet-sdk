import assert from 'node:assert/strict';
import test from 'node:test';
import { fixBlofinAccount, fixBybitRejectReasons, fixWeexConditionals } from './known-ts-fixes.mjs';

test('conditional oracle uses taped trigger and exposes an incorrect Rust price', () => {
  const account = { orders: [{ id: '702345678901234565', type: 'stop_market', reduceOnly: false, price: 0 }] };
  const records = [{ t: 1, type: 'ws', text: JSON.stringify({ e: 'orders', d: [{ id: account.orders[0].id, type: 'STOP_MARKET', triggerPrice: '71000', price: '0' }] }) }];
  fixWeexConditionals(account, records);
  assert.equal(account.orders[0].type, 'conditional');
  assert.equal(account.orders[0].price, 71000);
  assert.notDeepEqual(account, { orders: [{ ...account.orders[0], price: 1 }] });
});

test('REST seed and later stream update supply independent expected prices', () => {
  const account = { orders: [{ id: '1', type: 'conditional', price: 0 }] };
  fixWeexConditionals(account, [
    { t: 0, type: 'http', method: 'GET', url: 'https://example.test/openAlgoOrders', status: 200, body: '[{"algoId":"1","orderType":"STOP","triggerPrice":"70000"}]' },
    { t: 2, type: 'ws', text: '{"e":"orders","d":[{"id":"1","type":"STOP","triggerPrice":"71000"}]}' },
  ]);
  assert.equal(account.orders[0].price, 71000);
});

test('missing oracle and protective rows are not rewritten', () => {
  const account = { orders: [{ id: '1', type: 'stop_market', price: 0 }, { id: '2', type: 'stop_market', reduceOnly: true, price: 68000 }] };
  const before = structuredClone(account);
  fixWeexConditionals(account, [{ t: 1, type: 'http', method: 'POST', url: 'https://example.test/algoOrder', status: 200, requestBody: '{"type":"STOP_MARKET","triggerPrice":"68000","reduceOnly":true}', body: '{"orderId":"2"}' }]);
  assert.deepEqual(account, before);
});

test('all native protective markers prevent conditional rewriting', () => {
  for (const marker of [{ algoType: 'TPSL' }, { algoType: 'TP_SL' }, { positionTpsl: true }]) {
    const account = { orders: [{ id: '1', type: 'stop_market', price: 68000 }] };
    fixWeexConditionals(account, [{ t: 1, type: 'ws', text: JSON.stringify({ e: 'orders', d: [{ id: '1', type: 'STOP_MARKET', triggerPrice: '71000', ...marker }] }) }]);
    assert.equal(account.orders[0].type, 'stop_market');
    assert.equal(account.orders[0].price, 68000);
  }
});

test('numeric 18-digit identifiers retain their exact recorded spelling', () => {
  const account = { orders: [{ id: '702345678901234565', type: 'conditional', price: 0 }] };
  fixWeexConditionals(account, [{ t: 1, type: 'ws', text: '{"e":"orders","d":[{"id":702345678901234565,"type":"STOP_MARKET","triggerPrice":"71000"}]}' }]);
  assert.equal(account.orders[0].price, 71000);
});


test('BloFin unavailable margin cannot be defined by a wrong positive Rust margin', () => {
  const account = { positions: [{ symbol: 'BTCUSDT', side: 'long', marginSize: 0,
    contracts: 0.004, entryPrice: 59965.8, notional: 240, leverage: 30 }], balance: { used: 10, upnl: 2 } };
  const wrongRust = structuredClone(account);
  wrongRust.positions[0].marginSize = 123456;
  fixBlofinAccount(account, wrongRust);
  assert.equal(account.positions[0].marginSize, null);
  assert.notDeepEqual(account, wrongRust);
  assert.equal(wrongRust.positions[0].marginSize, 123456);
  // A plausible notional/leverage value is also unsupported per-position margin.
  wrongRust.positions[0].marginSize = 8;
  assert.notDeepEqual(account, wrongRust);
});

test('BloFin only repairs placeholder zero and preserves explicit TS margin values', () => {
  const account = { positions: [{ marginSize: 0 }, { marginSize: null }, { marginSize: 25 }], balance: { used: 10, upnl: 2 } };
  fixBlofinAccount(account);
  assert.deepEqual(account.positions, [{ marginSize: null }, { marginSize: null }, { marginSize: 25 }]);
  assert.deepEqual(account.balance, { used: 10, upnl: 2 });
  account.positions = [];
  fixBlofinAccount(account);
  assert.deepEqual(account.balance, { used: 0, upnl: 0 });
});

test('Bybit rejection correction stays independent of an incorrect Rust result', () => {
  const account = { orders: [{ id: '1', status: 'canceled', rejectReason: null }] };
  const wrongRust = structuredClone(account);
  const records = [{ type: 'ws', text: '{"topic":"order","data":[{"orderId":"1","rejectReason":"EC_PostOnlyWillTakeLiquidity"}]}' }];
  fixBybitRejectReasons(account, records);
  assert.equal(account.orders[0].rejectReason, 'PostOnly');
  assert.notDeepEqual(account, wrongRust);
  const ordinary = { orders: [{ id: '1', status: 'canceled', rejectReason: null }] };
  wrongRust.orders[0].rejectReason = 'PostOnly';
  fixBybitRejectReasons(ordinary, []);
  assert.equal(ordinary.orders[0].rejectReason, null);
  assert.notDeepEqual(ordinary, wrongRust);
});

test('Bybit latest recorded rejection and existing reason constrain the correction', () => {
  const account = { orders: [
    { id: '1', status: 'canceled', rejectReason: null },
    { id: '2', status: 'open', rejectReason: null },
    { id: '3', status: 'canceled', rejectReason: 'Existing' },
    { id: '702345678901234565', status: 'canceled', rejectReason: null },
  ] };
  fixBybitRejectReasons(account, [
    { type: 'ws', text: '{"topic":"order","data":[{"orderId":"1","rejectReason":"EC_PostOnlyWillTakeLiquidity"}]}' },
    { type: 'ws', text: '{"topic":"order","data":[{"orderId":"1","rejectReason":"EC_PerCancelRequest"},{"orderId":"2","rejectReason":"EC_PostOnlyWillTakeLiquidity"},{"orderId":"3","rejectReason":"EC_PostOnlyWillTakeLiquidity"},{"orderId":702345678901234565,"rejectReason":"EC_PostOnlyWillTakeLiquidity"}]}' },
  ]);
  assert.deepEqual(account.orders.map((order) => order.rejectReason), [null, null, 'Existing', 'PostOnly']);
});
