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
// Immediate fake routes, analogous to Rust test_support::prepared_actions.
// A final canonical packet becomes observable only after the real engine claim.
function released(engine, wire, now = 1000) {
  const queue = decode(wire), result = [], resources = new Map();
  let next = 0n, count = 0;
  while (queue.length) {
    assert.ok(++count <= 65_536, 'Entropy fixture action loop exceeded its bound');
    const action = queue.shift();
    assert.ok(action.op !== 'sign' && action.op !== 'signPayload', 'Entropy fixture requires a local signer');
    if (action.op !== 'managedHttp') { result.push(action); continue; }
    if (action.phase === 'prepare') {
      const preparation = tokenFields(action.preparation, 'p1', 3);
      assert.equal(preparation[0], String(action.packet.request));
      assert.ok(engine.httpPreparationPending(action.preparation));
      assert.ok(!resources.has(action.preparation), 'Prepared resource cannot be replaced');
      const resource = String(++next);
      resources.set(action.preparation, { request: action.packet.request, resource });
      queue.unshift(...decode(engine.httpPrepared(action.preparation, resource, undefined, now)));
    } else if (action.phase === 'bind') {
      const resource = resources.get(action.preparation);
      assert.ok(resource, 'Binding requires the exact prepared resource');
      assert.equal(resource.request, action.packet.request);
      const preparation = tokenFields(action.preparation, 'p1', 3);
      const permit = tokenFields(action.permit, 'r1', 4);
      assert.deepEqual(permit.slice(0, 2), preparation.slice(0, 2));
      assert.equal(permit[0], String(action.packet.request));
      assert.equal(permit[2], resource.resource);
      resources.delete(action.preparation);
      const claim = JSON.parse(engine.httpClaim(action.permit, now));
      assert.ok(claim.status === 'released' || claim.status === 'denied', 'Paced/parked entropy routes are unsupported');
      if (claim.status === 'released') result.push({ ...action.packet, op: 'http' });
      // Claim is scalar; drain only after the fixture's physical handoff.
      queue.unshift(...decode(engine.prepareDispatch(now)));
    } else if (action.phase === 'cancel') {
      resources.delete(action.preparation);
    } else assert.fail('Unexpected managed entropy action');
  }
  assert.equal(resources.size, 0, 'Parked entropy routes are unsupported');
  return result;
}
function tokenFields(value, prefix, count) {
  assert.equal(typeof value, 'string');
  const parts = value.split(':');
  assert.equal(parts.shift(), prefix);
  assert.equal(parts.length, count);
  for (const part of parts) {
    assert.match(part, /^(?:0|[1-9][0-9]*)$/);
    assert.ok(part.length < 20 || (part.length === 20 && part <= '18446744073709551615'));
  }
  return parts;
}
function orders(source) {
  const engine = new WasmEngine('binance', JSON.stringify({ key: 'fixture-key', secret: 'fixture-secret' }));
  try {
    if (source) engine.setEntropy(source);
    const actions = released(engine, engine.start(1000));
    const append = (wire) => {
      const next = released(engine, wire);
      assert.ok(actions.length + next.length <= 65_536, 'Entropy startup exceeded its total action bound');
      actions.push(...next);
    };
    for (let i = 0; i < actions.length; i++) {
      assert.ok(i < 65_536, 'Entropy startup action loop exceeded its bound');
      const action = actions[i];
      if (action.op !== 'http') continue;
      const body = responses[new URL(action.url).pathname.split('/').pop()];
      if (!body) continue;
      append(engine.httpStart(action.request, 200, '', 1000));
      append(engine.httpChunk(action.request, new TextEncoder().encode(body), 1000));
      append(engine.httpEnd(action.request, 1000));
    }
    assert.throws(() => engine.setEntropy(seeded(2)), (error) => JSON.parse(error.venueError).kind === 'invalidInput');
    const result = released(engine, engine.command(JSON.stringify({ op: 'placeOrders', command: 1, orders: [
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

test('raw entropy fixture rejects mismatched attempt/resource tokens before claiming', () => {
  const preparation = 'p1:7:9007199254740993:1';
  const packet = { request: 7, method: 'POST', url: 'https://venue.invalid/order', headers: [] };
  for (const permit of ['r1:7:9007199254740992:1:1', 'r1:7:9007199254740993:2:1', 'r1:07:9007199254740993:1:1']) {
    let claims = 0;
    const engine = {
      httpPreparationPending: () => true,
      httpPrepared: () => JSON.stringify([{ op: 'managedHttp', phase: 'bind', preparation, permit, packet }]),
      httpClaim() { claims++; return '{"status":"released"}'; },
      prepareDispatch: () => '',
    };
    assert.throws(() => released(engine, JSON.stringify([{ op: 'managedHttp', phase: 'prepare', preparation, packet }])));
    assert.equal(claims, 0);
  }
});

test('raw entropy fixture exposes the exact final packet only after a successful claim', () => {
  const preparation = 'p1:7:9007199254740993:1';
  const packet = { request: 7, method: 'POST', url: 'https://venue.invalid/order?signature=final', headers: [['key', 'fixture']] };
  for (const status of ['released', 'denied']) {
    const events = [];
    const engine = {
      httpPreparationPending: () => true,
      httpPrepared: () => JSON.stringify([{ op: 'managedHttp', phase: 'bind', preparation, permit: 'r1:7:9007199254740993:1:1', packet }]),
      httpClaim() { events.push('claim'); return JSON.stringify({ status }); },
      prepareDispatch() { events.push('drain'); return ''; },
    };
    const actions = released(engine, JSON.stringify([{ op: 'managedHttp', phase: 'prepare', preparation, packet: { ...packet, url: 'https://venue.invalid/order', headers: [] } }]));
    assert.deepEqual(events, ['claim', 'drain']);
    assert.deepEqual(actions, status === 'released' ? [{ ...packet, op: 'http' }] : []);
  }
});
