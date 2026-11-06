import assert from 'node:assert/strict';
import test from 'node:test';
import { venueMarketRules } from './live-smoke-rules.mjs';
import { smokeOrder } from './live-smoke.mjs';
import { MARKET_FLAG } from '../hosts/wasm/js/wire.mjs';

const market = { id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', flags: MARKET_FLAG.ACTIVE,
  precisionPrice: 0.1, precisionAmount: 0.001, minAmount: 0.001, maxAmount: 1000 };
const row = { symbol: 'BTCUSDT', type: 'PerpetualV2', status: 'Listed', quoteCurrency: 'USDT', settleCurrency: 'USDT',
  minOrderValueRv: '1', qtyStepSize: '0.001', maxOrderQtyRq: '1000' };
const record = (value = row, host = 'api.phemex.com', extra = {}) => ({ type: 'http', method: 'GET', status: 200,
  url: `https://${host}/public/products`, body: JSON.stringify({ code: 0, data: { perpProductsV2: [value] } }), ...extra });
const rules = (records) => venueMarketRules('phemex', records, market);
const unknown = { notionalKnown: false, minNotional: 0 };

for (const host of ['api.phemex.com', 'testnet-api.phemex.com']) {
  test(`Phemex ${host} uses the explicit monetary floor and base-quantity limits`, () => {
    assert.deepEqual(rules([record(row, host)]), { notionalKnown: true, minNotional: 1,
      minimumAmount: 0.001, amountStep: 0.001, notionalPerAmount: 1, quoteQuantity: false, maximumAmount: 1000 });
  });
}

test('Phemex refuses missing or malformed monetary, step and maximum fields', () => {
  for (const field of ['minOrderValueRv', 'qtyStepSize', 'maxOrderQtyRq']) {
    for (const value of [undefined, null, '', 'NaN', 'Infinity', '-1', {}, true]) {
      const bad = { ...row, [field]: value };
      assert.deepEqual(rules([record(bad)]), unknown, `${field}/${JSON.stringify(value)}`);
    }
  }
  for (const field of ['qtyStepSize', 'maxOrderQtyRq']) assert.deepEqual(rules([record({ ...row, [field]: '0' })]), unknown);
  assert.deepEqual(rules([record({ ...row, qtyStepSize: '0.1', maxOrderQtyRq: '0.01' })]), unknown);
});

test('Phemex cannot borrow spot, inverse, foreign, failed or stale product limits', () => {
  for (const change of [{ symbol: 'ETHUSDT' }, { type: 'Spot' }, { status: 'Delisted' },
    { quoteCurrency: 'USD' }, { settleCurrency: 'BTC' }]) {
    assert.deepEqual(rules([record({ ...row, ...change })]), unknown);
  }
  for (const change of [{ method: 'POST' }, { status: 500 }, { url: 'https://evil.example/public/products' },
    { url: 'https://api.phemex.com/public/other' }, { body: '{' },
    { body: JSON.stringify({ code: 1, data: { perpProductsV2: [row] } }) },
    { body: JSON.stringify({ code: 0, data: { products: [row] } }) },
    { body: JSON.stringify({ code: 0, data: { perpProductsV2: [row, row] } }) }]) {
    assert.deepEqual(rules([record(row, undefined, change)]), unknown);
  }
  assert.deepEqual(rules([record(), record({ ...row, minOrderValueRv: null })]), unknown);
});

test('Phemex smoke sizing honors both explicit notional and the fixed risk cap', () => {
  const book = { bids: [50000], asks: [50001] };
  const limits = rules([record({ ...row, minOrderValueRv: '50' })]);
  const order = smokeOrder(market, book, limits);
  assert.equal(order.amount, 0.002);
  assert(order.amount * order.amendPrice >= 50);
  assert(order.amount * order.price <= 100);
  assert.equal(order.timeInForce, 'PostOnly');
  assert.throws(() => smokeOrder(market, book, rules([record({ ...row, minOrderValueRv: '101' })])), /notional-cap-exceeded/);
  assert.throws(() => smokeOrder(market, book, rules([record({ ...row, minOrderValueRv: null })])), /unknown-minimum-notional/);
});
