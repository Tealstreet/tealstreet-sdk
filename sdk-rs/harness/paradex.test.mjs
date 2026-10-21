import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { requestKey, subscriptionKey } from './requests.mjs';
import { Tape } from './tape.mjs';
import { venueOf } from './venues.mjs';
const venue = venueOf('paradex');
const vectors = JSON.parse(readFileSync(new URL('../venues/paradex/tests/sign-vectors.json', import.meta.url)));
const binary = JSON.parse(readFileSync(new URL('../../packages/safe-cex/src/__tests__/fixtures/paradex-sbe.json', import.meta.url)));
const SOCKET = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2';
const withNow = (time, fn) => { const previous = Date.now; Date.now = () => time; try { return fn(); } finally { Date.now = previous; } };
for (const c of vectors.cases) {
  const root = `https://api.${c.environment === 'prod' ? 'prod' : 'testnet'}.paradex.trade/v1`;
  test(`Paradex ${c.environment} verifies official auth typed data and rejects route/time/account/signature tampering`, () => withNow(c.now, () => {
    const timestamp = Math.floor(c.now / 1000);
    const request = { method: 'POST', url: `${root}/auth/0x456`, headers: {
      'PARADEX-STARKNET-ACCOUNT': c.address, 'PARADEX-STARKNET-SIGNATURE': c.auth.signature,
      'PARADEX-TIMESTAMP': String(timestamp), 'PARADEX-SIGNATURE-EXPIRATION': String(timestamp + 86400),
    } };
    assert.equal(venue.verifyRequest(request), true);
    assert.doesNotThrow(() => venue.createRequestAudit('ts').validate(request));
    assert.throws(() => venue.createRequestAudit('rust').validate(request), /auth lifetime/);
    const short = { ...request, headers: { ...request.headers,
      'PARADEX-STARKNET-SIGNATURE': c.shortAuth.signature,
      'PARADEX-SIGNATURE-EXPIRATION': String(timestamp + 300),
    } };
    assert.equal(venue.verifyRequest(short), true);
    assert.doesNotThrow(() => venue.createRequestAudit('rust').validate(short));
    assert.throws(() => venue.createRequestAudit('ts').validate(short), /auth lifetime/);
    assert.equal(venue.verifyRequest({ ...short, headers: { ...short.headers,
      'PARADEX-SIGNATURE-EXPIRATION': String(timestamp + 301),
    } }), false);
    assert.equal(venue.verifyRequest({ ...request, url: `${root}/auth/0x457` }), false);
    assert.equal(venue.verifyRequest({ ...request, body: '{}' }), false);
    assert.equal(venue.verifyRequest({ ...request, headers: { ...request.headers, 'PARADEX-STARKNET-ACCOUNT': '0x999' } }), false);
    assert.equal(venue.verifyRequest({ ...request, headers: { ...request.headers, 'PARADEX-TIMESTAMP': String(timestamp + 1) } }), false);
    assert.equal(venue.verifyRequest({ ...request, headers: { ...request.headers, 'PARADEX-STARKNET-SIGNATURE': '["1","1"]' } }), false);
  }));
  for (const order of c.orders) test(`Paradex ${c.environment} ${order.name} signature binds exact quantum and modify identity`, () => withNow(c.now, () => {
    const request = { method: order.id ? 'PUT' : 'POST', url: `${root}/orders${order.id ? `/${order.id}` : ''}`,
      headers: { Authorization: 'Bearer replay-paradex-jwt', 'PARADEX-PARTNER': 'TEALSTREET' },
      body: JSON.stringify({ market: order.market, side: order.side, type: order.type, size: order.size,
        ...(order.price === undefined ? {} : { price: order.price }), signature_timestamp: c.now, signature: order.signature }) };
    assert.equal(venue.verifyRequest(request), true);
    const tampered = JSON.parse(request.body); tampered.size = '987654321.12345678';
    assert.equal(venue.verifyRequest({ ...request, body: JSON.stringify(tampered) }), false);
    assert.equal(venue.verifyRequest({ ...request, headers: {} }), false);
  }));
}
test('Paradex matching retains every economic field and caller ID, normalizing only verified signing fields and generated IDs', () => {
  const caller = new Set(['caller-exact']);
  const key = (body) => requestKey('POST', 'https://api.prod.paradex.trade/v1/orders/batch', JSON.stringify(body), caller, venue);
  const row = { market: 'BTC-USD-PERP', side: 'BUY', type: 'LIMIT', size: '0.01', price: '50000', client_id: 'ts_random', signature: '["1","2"]', signature_timestamp: 1 };
  assert.equal(key([row]), key([{ ...row, client_id: 'ts_other', signature: '["3","4"]', signature_timestamp: 2 }]));
  for (const patch of [{ size: '0.02' }, { price: '50001' }, { side: 'SELL' }, { market: 'ETH-USD-PERP' }, { client_id: 'caller-exact' }]) assert.notEqual(key([row]), key([{ ...row, ...patch }]));
});
test('Paradex response echoes bind actual batch client IDs and accepted writes cannot be reused', () => {
  const url = 'https://api.prod.paradex.trade/v1/orders/batch';
  const tape = new Tape([{ type: 'http', method: 'POST', url, requestBody: '[{"client_id":"generated","size":"1"}]',
    status: 200, body: '{"orders":[{"id":"1","client_id":"$request:0.client_id"}]}' }], venue);
  const body = '[{"client_id":"ts_actual","size":"1"}]';
  assert.equal(tape.respond('POST', url, body).body, '{"orders":[{"id":"1","client_id":"ts_actual"}]}');
  assert.equal(tape.respond('POST', url, body).status, 599);
});
test('Paradex socket keys keep required channels, normalize RPC IDs, and omit protocol heartbeats', () => {
  const message = { jsonrpc: '2.0', method: 'subscribe', id: 0, params: { channel: 'orders.ALL' } };
  const key = (m) => subscriptionKey(SOCKET, m, new Set(), venue);
  assert.equal(key(message), key({ ...message, id: 22 }));
  assert.notEqual(key(message), key({ ...message, params: { channel: 'fills.ALL' } }));
  assert.equal(key({ type: 'ping' }), undefined);
});
test('binary tapes deliver exact bytes to matching shared-URL subscriptions despite different connection ordinals', async () => {
  const book = binary.cases.find((row) => row.name === 'book-snapshot');
  const fill = binary.cases.find((row) => row.name === 'fill-v2');
  const records = [book, fill].map((row, t) => ({ type: 'ws', t, url: SOCKET, socket: t, binary: row.base64 }));
  const tape = new Tape(records, { ...venue, createSocketAudit: undefined, publicTickers: undefined });
  const seen = [];
  const connect = (role) => tape.connect(SOCKET, () => {}, (data) => { if (typeof data !== 'string') seen.push([role, Buffer.from(data).toString('base64')]); });
  const privateSocket = connect('private');
  const publicSocket = connect('public');
  tape.send(privateSocket, JSON.stringify({ jsonrpc: '2.0', method: 'subscribe', id: 1, params: { channel: 'fills.ALL' } }));
  tape.send(publicSocket, JSON.stringify({ jsonrpc: '2.0', method: 'subscribe', id: 2, params: { channel: 'order_book.BTC-USD-PERP.deltas' } }));
  await tape.play();
  assert.deepEqual(seen, [['public', book.base64], ['private', fill.base64]]);
});
test('Paradex socket audit rejects private topics before authentication and data before subscription', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: [], tickers: [] });
  const socket = {};
  audit.sent(SOCKET, JSON.stringify({ method: 'subscribe', params: { channel: 'orders.ALL' } }), socket);
  audit.received(SOCKET, JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: { channel: 'fills.ALL', data: {} } }), socket);
  assert(audit.errors().some((e) => e.includes('before authentication')));
  assert(audit.errors().some((e) => e.includes('Unsubscribed')));
});

test('Paradex taped control rejection fails closed instead of synthesizing an accepted subscription', () => {
  const record = { type: 'ws', t: 0, url: SOCKET, text: JSON.stringify({ jsonrpc: '2.0', id: 12,
    error: { code: 400, message: 'subscription rejected' } }) };
  assert.throws(() => new Tape([record], venue), /Recorded Paradex control rejection/);
});

test('shared-URL lifecycle records close and reopen the recorded public role when actual socket order reverses', async () => {
  const book = binary.cases.find((row) => row.name === 'book-snapshot').base64;
  const fill = binary.cases.find((row) => row.name === 'fill-v2').base64;
  const records = [
    { type: 'open', t: 0, url: SOCKET, socket: 0 },
    { type: 'open', t: 1, url: SOCKET, socket: 1 },
    { type: 'ws', t: 2, url: SOCKET, socket: 0, binary: book },
    { type: 'ws', t: 3, url: SOCKET, socket: 1, binary: fill },
    { type: 'close', t: 4, url: SOCKET, socket: 0, code: 1006 },
    { type: 'open', t: 5, url: SOCKET, socket: 2 },
    { type: 'ws', t: 6, url: SOCKET, socket: 2, binary: book },
    { type: 'ws', t: 7, url: SOCKET, socket: 1, binary: fill },
  ];
  const tape = new Tape(records, { ...venue, createSocketAudit: undefined, publicTickers: undefined });
  const seen = [];
  const connect = (role, generation = 0) => {
    const socket = tape.connect(SOCKET, () => {}, (data) => { if (typeof data !== 'string') seen.push(`${role}${generation}:data`); },
      () => { seen.push(`${role}${generation}:close`); if (role === 'public') connect(role, generation + 1); });
    tape.send(socket, JSON.stringify({ method: 'subscribe', id: generation,
      params: { channel: role === 'public' ? 'order_book.BTC-USD-PERP.deltas' : 'fills.ALL' } }));
    return socket;
  };
  const privateSocket = connect('private');
  connect('public');
  await tape.play();
  assert.deepEqual(seen, ['public0:data', 'private0:data', 'public0:close', 'public1:data', 'private0:data']);
  assert.equal(privateSocket.closed, false);
});

test('Paradex account settings read the configured cache independently of effective position leverage', () => {
  const store = { markets: [{ symbol: 'BTC-USD-PERP' }, { symbol: 'ETH-USD-PERP' }],
    positions: [{ symbol: 'BTC-USD-PERP', leverage: 5, marginType: 'cross' }] };
  const exchange = { leverageSettings: new Map([['BTC-USD-PERP', 10], ['ETH-USD-PERP', 20]]) };
  assert.deepEqual(venue.tsSettings(exchange, store), [
    { symbol: 'BTC-USD-PERP', marginType: 'cross', leverage: 10 },
    { symbol: 'ETH-USD-PERP', marginType: 'cross', leverage: 20 },
  ]);
  exchange.leverageSettings.set('BTC-USD-PERP', 30);
  assert.equal(venue.tsSettings(exchange, store)[0].leverage, 30);
  exchange.leverageSettings.clear();
  assert.equal(venue.tsSettings(exchange, store)[0].leverage, 10);
});
