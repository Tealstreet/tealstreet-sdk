// Runs against the freshly built combined WASM engine in the existing CI host job.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/safe_cex_wasm.js');
const decode = (value) => value ? JSON.parse(value) : [];
const seeded = (seed) => (length) => Uint8Array.from({ length }, () => seed++ & 255);
const info = JSON.stringify({ symbols: [{ symbol: 'BTCUSDT', status: 'TRADING', baseAsset: 'BTC',
  quoteAsset: 'USDT', marginAsset: 'USDT', contractType: 'PERPETUAL', filters: [
    { filterType: 'PRICE_FILTER', tickSize: '0.1' },
    { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
    { filterType: 'MARKET_LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
  ] }] });
const responses = { exchangeInfo: info,
  '24hr': '[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]',
  bookTicker: '[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]',
  premiumIndex: '[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0","nextFundingTime":1700000000000}]' };
function orders(source) {
  const engine = new WasmEngine('binance', JSON.stringify({ key: 'fixture-key', secret: 'fixture-secret' }));
  try {
    if (source) engine.setEntropy(source);
    const actions = decode(engine.start(1000));
    for (let i = 0; i < actions.length; i++) {
      const action = actions[i];
      if (action.op !== 'http') continue;
      const body = responses[new URL(action.url).pathname.split('/').pop()];
      if (!body) continue;
      actions.push(...decode(engine.httpStart(action.request, 200, '', 1000)));
      actions.push(...decode(engine.httpChunk(action.request, new TextEncoder().encode(body), 1000)));
      actions.push(...decode(engine.httpEnd(action.request, 1000)));
    }
    assert.throws(() => engine.setEntropy(seeded(2)), (error) => JSON.parse(error.venueError).kind === 'invalidInput');
    const result = decode(engine.command(JSON.stringify({ op: 'placeOrders', command: 1, orders: [
      { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 1, price: 100 },
    ] }), 1000));
    return result;
  } finally { engine.free(); }
}
const ids = (actions) => actions.filter((a) => a.op === 'http').map((a) => new URL(a.url).searchParams.get('newClientOrderId'));

// Must precede the first construction: js-sys caches its chosen global object.
test('real WASM does not inspect shim getters when realm WebCrypto is available', () => {
  const descriptors = ['self', 'window'].map((key) => Object.getOwnPropertyDescriptor(globalThis, key));
  for (const key of ['self', 'window'])
    Object.defineProperty(globalThis, key, { configurable: true, get() { throw new Error('Unexpected shim access'); } });
  try {
    const engine = new WasmEngine('binance', '{"publicDataOnly":true}');
    engine.free();
  } finally {
    ['self', 'window'].forEach((key, index) => {
      if (descriptors[index]) Object.defineProperty(globalThis, key, descriptors[index]);
      else delete globalThis[key];
    });
  }
});

test('real WASM uses realm WebCrypto when self and window are empty shims', () => {
  const descriptors = ['self', 'window'].map((key) => Object.getOwnPropertyDescriptor(globalThis, key));
  for (const key of ['self', 'window'])
    Object.defineProperty(globalThis, key, { configurable: true, value: {} });
  try {
    const engine = new WasmEngine('binance', '{"publicDataOnly":true}');
    engine.free();
  } finally {
    ['self', 'window'].forEach((key, index) => {
      if (descriptors[index]) Object.defineProperty(globalThis, key, descriptors[index]);
      else delete globalThis[key];
    });
  }
});

test('real WASM client IDs repeat with a source and remain random in production', () => {
  const first = ids(orders(seeded(7)));
  assert.equal(first.length, 1);
  assert.ok(first[0]);
  assert.deepEqual(first, ids(orders(seeded(7))));
  assert.notDeepEqual(first, ids(orders(seeded(8))));
  assert.notDeepEqual(ids(orders()), ids(orders()));
});

test('real WASM validates callback exceptions, type and exact length before an order write', () => {
  for (const source of [() => { throw new Error('PRIVATE_DETAIL'); }, () => [], () => new Uint8Array(1)]) {
    const result = orders(source);
    assert.deepEqual(ids(result), []);
    const failed = result.find((a) => a.op === 'done' && a.error);
    assert.ok(failed, JSON.stringify(result));
    assert.ok(!JSON.stringify(result).includes('PRIVATE_DETAIL'));
  }
});

test('real WASM refuses construction without cryptographic host providers', () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'crypto');
  Object.defineProperty(globalThis, 'crypto', { configurable: true, value: undefined });
  try {
    assert.throws(() => new WasmEngine('binance', '{"publicDataOnly":true}'),
      (error) => JSON.parse(error.venueError).kind === 'invalidInput');
  } finally { Object.defineProperty(globalThis, 'crypto', original); }
});

test('raw WASM setters reject missing and non-callable providers before startup', () => {
  const engine = new WasmEngine('binance', '{"publicDataOnly":true}');
  try {
    for (const source of [undefined, null, {}, 7]) {
      for (const setter of ['setEntropy', 'setSigningEntropy']) {
        assert.throws(() => engine[setter](source),
          (error) => JSON.parse(error.venueError).kind === 'invalidInput');
      }
    }
  } finally { engine.free(); }
});
