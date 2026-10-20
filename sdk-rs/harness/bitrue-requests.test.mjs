import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { requestKey, subscriptionKeys } from './requests.mjs';
import { venueOf } from './venues.mjs';
import { Tape } from './tape.mjs';
const venue = venueOf('bitrue');
const base = 'https://fapi.bitrue.com';
const key = (path, body, callerIds = new Set()) => requestKey('POST', base + path, body, callerIds, venue);
test('Bitrue normalizes minted create values while retaining caller IDs and native field presence', () => {
  const create = '/fapi/v2/order';
  assert.equal(
    key(create, { clientOrderId: 'minted-1', volume: '0.004' }),
    key(create, { clientOrderId: 'minted-2', volume: '0.004' }),
  );
  assert.notEqual(key(create, { clientOrderId: 'minted-1' }), key(create, {}));
  const caller = new Set(['caller-id']);
  assert.notEqual(
    key(create, { clientOrderId: 'caller-id' }, caller),
    key(create, { clientOrderId: 'changed' }, caller),
  );
  const child = (clientOrderId) => ({ triggerOrderCreateParams: [{ clientOrderId, triggerPrice: 55000 }] });
  assert.equal(key(create, child('minted-child-1')), key(create, child('minted-child-2')));
  assert.notEqual(
    key(create, child('minted-child-1')),
    key(create, { triggerOrderCreateParams: [{ triggerPrice: 55000 }] }),
  );
});
test('Bitrue cancellation preserves the exact server client ID and business wire types', () => {
  const cancel = '/fapi/v2/cancel';
  assert.notEqual(key(cancel, { clientOrderId: 'server-id-1' }), key(cancel, { clientOrderId: 'server-id-2' }));
  assert.notEqual(key(cancel, { clientOrderId: 'server-id-1' }), key(cancel, {}));
  assert.notEqual(key('/fapi/v2/order', { volume: '0.004' }), key('/fapi/v2/order', { volume: 0.004 }));
});
test('Bitrue rejects unsigned private reads and writes; signatures bind exact route and body', () => {
  for (const [method, path] of [
    ['POST', '/fapi/v2/order'],
    ['GET', '/fapi/v2/myTrades'],
    ['GET', '/fapi/v2/account'],
  ]) {
    assert.throws(() => venue.validateRequest({ method, url: base + path, body: '{}', headers: {} }), /signature/);
  }
  assert.throws(
    () =>
      venue.validateRequest({
        method: 'POST',
        url: 'https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey',
        headers: {},
      }),
    /signature/,
  );
  assert.doesNotThrow(() => venue.validateRequest({ method: 'GET', url: base + '/fapi/v1/contracts', headers: {} }));
  const timestamp = String(Date.now()),
    method = 'POST',
    path = '/fapi/v2/order',
    body = '{"volume":"0.004"}';
  const headers = {
    'X-CH-APIKEY': 'replay-key',
    'X-CH-TS': timestamp,
    'X-CH-SIGN': createHmac('sha256', 'replay-secret')
      .update(timestamp + method + path + body)
      .digest('hex'),
  };
  assert.doesNotThrow(() => venue.validateRequest({ method, url: base + path, body, headers }));
  assert.throws(() => venue.validateRequest({ method, url: base + path, body: '{}', headers }), /signature/);
});

test('the extracted Tape selects Bitrue typed matching with route-scoped generated IDs', () => {
  const path = '/fapi/v2/order',
    method = 'POST',
    timestamp = String(Date.now());
  const body = '{"clientOrderId":"actual-minted","volume":"0.004"}';
  const headers = {
    'X-CH-APIKEY': 'replay-key',
    'X-CH-TS': timestamp,
    'X-CH-SIGN': createHmac('sha256', 'replay-secret')
      .update(timestamp + method + path + body)
      .digest('hex'),
  };
  const tape = new Tape(
    [
      {
        type: 'http',
        method,
        url: base + path,
        status: 200,
        body: 'accepted',
        requestBody: { clientOrderId: 'taped-minted', volume: '0.004' },
      },
    ],
    venue,
    true,
    30000,
    'rust',
  );
  assert.equal(tape.respond(method, base + path, body, headers).body, 'accepted');
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.respond(method, base + path, body, headers).status, 599);
});

test('an invalid signed read remains a gate failure after a valid retry', () => {
  const method = 'GET',
    path = '/fapi/v2/account',
    timestamp = String(Date.now());
  const tape = new Tape(
    [{ type: 'http', method, url: base + path, status: 200, body: '{"code":"0","data":{"account":[]}}' }],
    venue,
    true,
    30000,
    'rust',
  );
  assert.throws(() => tape.respond(method, base + path, undefined, {}), /signature/);
  const headers = {
    'X-CH-APIKEY': 'replay-key',
    'X-CH-TS': timestamp,
    'X-CH-SIGN': createHmac('sha256', 'replay-secret')
      .update(timestamp + method + path)
      .digest('hex'),
  };
  assert.equal(tape.respond(method, base + path, undefined, headers).status, 200);
  assert.equal(tape.invalidSignatures, 1);
  assert.equal(tape.unserved, 0);
});


test('Bitrue subscription identity retains channels while normalizing callback IDs and heartbeat clocks', () => {
  const url = 'wss://fmarket-ws.bitrue.com/kline-api/ws';
  const keys = (message) => subscriptionKeys(url, JSON.stringify(message), new Set(), venue);
  const sub = (channel, cb_id) => ({ event: 'sub', params: { channel, cb_id } });
  assert.deepEqual(keys(sub('market_e_btcusdt_ticker', 'ts-random')), keys(sub('market_e_btcusdt_ticker', 'engine')));
  assert.notDeepEqual(keys(sub('market_e_btcusdt_ticker', 'same')), keys(sub('market_e_ethusdt_ticker', 'same')));
  assert.notDeepEqual(keys(sub('market_e_btcusdt_ticker', 'same')), keys({ event: 'unsub', params: { channel: 'market_e_btcusdt_ticker', cb_id: 'same' } }));
  assert.deepEqual(keys({ pong: 123 }), []);
  assert.deepEqual(keys({ event: 'pong', ts: 456 }), []);
});
