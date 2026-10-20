import { test } from 'node:test';
import assert from 'node:assert/strict';
import { smokeArgs, smokeOrder, marketRules, fencedTransport } from './live-smoke.mjs';
import { MARKET_FLAG } from '../hosts/wasm/js/wire.mjs';
import { subscriptionKey } from './requests.mjs';
import { assertLiveSmokeComplete } from './live-smoke-proof.mjs';

const market = { symbol: 'BTCUSDT', id: 'BTC/USDT:USDT', flags: MARKET_FLAG.ACTIVE,
  precisionPrice: 0.1, precisionAmount: 0.001, minAmount: 0.001, maxAmount: 1000, minPrice: NaN, contractSize: NaN };
const book = { bids: [100000, 1, 1], asks: [100001, 1, 1] };

test('CLI never infers a mainnet request and accepts exactly one environment', () => {
  assert.equal(smokeArgs(['--venue', 'binance']).mode, undefined);
  assert.equal(smokeArgs(['--venue', 'binance', '--mainnet']).mode, 'mainnet');
  for (const args of [[], ['--venue', 'binance', '--demo', '--testnet'], ['--venue', 'binance', '--timeout-ms', 'NaN'], ['--venue', 'binance', '--secret', 'bad']])
    assert.throws(() => smokeArgs(args));
});

test('minimum resting order is aligned and covers notional at its lower amended price', () => {
  const order = smokeOrder(market, book, { minNotional: 100, lowerMultiplier: 0.8, notionalKnown: true });
  assert.deepEqual(order, { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.002, price: 90000, amendPrice: 89000 });
  assert.ok(order.amount * order.amendPrice >= 100);
  assert.ok((order.amount - market.precisionAmount) * order.amendPrice < 100);
});

test('inverse minimum notional uses quote contracts', () => {
  assert.equal(smokeOrder({ ...market, flags: MARKET_FLAG.ACTIVE | MARKET_FLAG.INVERSE,
    precisionAmount: 1, minAmount: 1, contractSize: 10 }, book, { minNotional: 21, notionalKnown: true }).amount, 3);
});

test('native lot rules override display quantities and bound the native maximum', () => {
  const rules = { notionalKnown: true, minNotional: 100, minimumAmount: 1, amountStep: 1,
    maximumAmount: 2, notionalPerAmount: 0.001, quoteQuantity: false };
  assert.equal(smokeOrder({ ...market, minAmount: 0.001, maxAmount: 0.002 }, book, rules).amount, 2);
  assert.throws(() => smokeOrder(market, book, { ...rules, minNotional: 200 }), /minimum-exceeds/);
  assert.equal(smokeOrder({ ...market, minAmount: 0 }, book, { ...rules, minNotional: 0 }).amount, 1);
});

test('invalid, stale-compatible shapes and no valid far price fail before orders', () => {
  for (const change of [{ flags: 0 }, { flags: 1 | MARKET_FLAG.READ_ONLY }, { precisionPrice: 0 }, { precisionAmount: NaN }, { minAmount: 0 }, { maxAmount: 0.0001 }])
    assert.throws(() => smokeOrder({ ...market, ...change }, book, { notionalKnown: true }));
  assert.throws(() => smokeOrder(market, { bids: [1], asks: [1] }, { notionalKnown: true }));
  assert.throws(() => smokeOrder(market, book, { lowerMultiplier: 0.99, notionalKnown: true }));
  assert.throws(() => smokeOrder({ ...market, maxAmount: 0.001 }, book, { minNotional: 1000, notionalKnown: true }));
  assert.throws(() => smokeOrder(market, book), /unknown-minimum-notional/);
});

test('instrument rules are selected by exact native or display market identity', () => {
  const records = [{ type: 'http', method: 'GET', url: 'https://fapi.binance.com/fapi/v1/exchangeInfo', status: 200, body: JSON.stringify({ symbols: [
    { symbol: 'ETHUSDT', filters: [{ filterType: 'MIN_NOTIONAL', notional: '99999' }] },
    { symbol: 'BTCUSDT', filters: [{ filterType: 'LOT_SIZE', minQty: '0.001', stepSize: '0.001', maxQty: '1000' },
      { filterType: 'MIN_NOTIONAL', notional: '100' }, { filterType: 'PERCENT_PRICE', multiplierDown: '0.8' }] },
  ] }) }];
  const rules = marketRules(records, market);
  assert.equal(rules.minNotional, 100);
  assert.equal(rules.lowerMultiplier, 0.8);
  assert.equal(rules.notionalKnown, true);
});

test('transport fences run before every HTTP, socket and outbound WS write', async () => {
  let calls = 0;
  const live = { request: () => calls++, connect: () => { calls++; return { send: () => calls++, close() {} }; } };
  const transport = fencedTransport(live, { allowUrl: (url) => url === 'https://test.example',
    validateRequest: () => { throw new Error('request-policy'); }, validateSocketMessage: () => { throw new Error('socket-policy'); } });
  assert.throws(() => transport.request({ url: 'https://mainnet.example' }), /refused/);
  assert.throws(() => transport.request({ url: 'https://test.example' }), /policy/);
  assert.equal(calls, 0);
  const socket = transport.connect('https://test.example', {});
  assert.throws(() => socket.send('bad'), /policy/);
  assert.equal(calls, 1);
});

test('private Binance liveness query is independent of real feed subscriptions', () => {
  assert.equal(subscriptionKey('wss://demo-fstream.binance.com/private/ws?listenKey=fake', { method: 'LIST_SUBSCRIPTIONS', id: 42 }), undefined);
  assert.match(subscriptionKey('wss://demo-fstream.binance.com/public/ws', { method: 'SUBSCRIBE', params: ['btcusdt@depth'], id: 42 }), /btcusdt@depth/);
});

test('failed/aborted captures cannot gate as completed live lifecycle proof', () => {
  const meta = { type: 'meta', liveSmoke: true };
  assert.throws(() => assertLiveSmokeComplete([meta]), /incomplete/);
  assert.throws(() => assertLiveSmokeComplete([meta, { type: 'live-smoke-result', ok: false }]), /failed/);
  assert.throws(() => assertLiveSmokeComplete([meta, { type: 'live-smoke-result', ok: true, cleanupComplete: true, authoritative: true, stage: 'complete', symbol: 'BTCUSDT', commands: [] },
    { type: 'live-smoke-verification', ok: true }]), /missing/);
  assert.doesNotThrow(() => assertLiveSmokeComplete([{ type: 'meta', private: true }]));
});
