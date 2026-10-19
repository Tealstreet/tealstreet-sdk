import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { venueOf } from './venues.mjs';
const venue = venueOf('cryptocom');
const privateUrl = 'wss://stream.crypto.com/exchange/v1/user';
const publicUrl = 'wss://stream.crypto.com/exchange/v1/market';
const auth = () => {
  const method = 'public/auth', id = Date.now(), nonce = id, api_key = 'replay-key';
  const sig = createHmac('sha256', 'replay-secret').update(method + id + api_key + nonce).digest('hex');
  return { method, id, nonce, api_key, sig };
};
const subscriptions = ['user.order', 'user.advance.order', 'user.positions', 'user.balance', 'user.trade'];
test('HMAC check binds the signed method to the REST route', () => {
  const body = (method) => {
    const id = Date.now(), nonce = id, api_key = 'replay-key', params = { client_oid: 'caller' };
    const sig = createHmac('sha256', 'replay-secret').update(method + id + api_key + 'client_oidcaller' + nonce).digest('hex');
    return JSON.stringify({ method, id, nonce, api_key, params, sig });
  };
  const url = 'https://api.crypto.com/exchange/v1/private/create-order';
  assert.equal(venue.checkRequest('POST', url, body('private/create-order')), true);
  assert.equal(venue.checkRequest('POST', url, body('private/wrong-method')), false);
  assert.equal(venue.checkRequest('GET', url, body('private/create-order')), false);
});
test('socket audit checks auth, subscriptions and heartbeat identity', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: ['BTCUSD-PERP'] });
  audit.sent(privateUrl, JSON.stringify(auth()));
  audit.received(privateUrl, JSON.stringify({ method: 'public/auth', code: 0 }));
  audit.sent(privateUrl, JSON.stringify({ method: 'subscribe', params: { channels: subscriptions } }));
  audit.sent(publicUrl, JSON.stringify({ method: 'subscribe', params: { channels: ['book.BTCUSD-PERP.50'] } }));
  audit.received(privateUrl, JSON.stringify({ method: 'public/heartbeat', id: 123 }));
  audit.sent(privateUrl, JSON.stringify({ method: 'public/respond-heartbeat', id: 123 }));
  assert.deepEqual(audit.errors(), []);
});
test('bad signatures and missing channels cannot pass the socket audit', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  audit.sent(privateUrl, JSON.stringify({ ...auth(), sig: 'invalid' }));
  audit.sent(privateUrl, JSON.stringify({ method: 'subscribe', params: { channels: subscriptions.slice(0, -1) } }));
  assert(audit.errors().some((e) => e.includes('signature')));
  assert(audit.errors().some((e) => e.includes('user.trade')));
});

test('HMAC check preserves exact nanosecond integer parameters', () => {
  const method = 'private/get-trades', id = Date.now(), nonce = id, api_key = 'replay-key';
  const payload = method + id + api_key + 'end_time1610000000000000001' + nonce;
  const sig = createHmac('sha256', 'replay-secret').update(payload).digest('hex');
  const body = JSON.stringify({ method, id, nonce, api_key, sig, params: {} }).replace('"params":{}', '"params":{"end_time":1610000000000000001}');
  const url = 'https://api.crypto.com/exchange/v1/private/get-trades';
  assert.equal(venue.checkRequest('POST', url, body), true);
  assert.equal(venue.checkRequest('POST', url, body.replace('1610000000000000001', '1610000000000000002')), false);
});

test('public audit requires ticker subscriptions before replay', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: false, books: ['BTCUSD-PERP'], tickers: ['BTCUSD-PERP', 'ETHUSD-PERP'] });
  const send = (channels) => audit.sent(publicUrl, JSON.stringify({ method: 'subscribe', params: { channels } }));
  send(['book.BTCUSD-PERP.50']);
  assert.equal(audit.ready(), false);
  assert(audit.errors().some((e) => e.includes('missing ticker subscription')));
  const ready = venue.createSocketAudit('rust', { isPrivate: false, books: [], tickers: ['BTCUSD-PERP'] });
  ready.sent(publicUrl, JSON.stringify({ method: 'subscribe', params: { channels: ['ticker.BTCUSD-PERP'] } }));
  assert.equal(ready.ready(), true);
  assert.deepEqual(ready.errors(), []);
});

test('public Crypto.com startup subscribes after the first socket opens', async () => {
  const calls = [];
  const exchange = {
    store: { markets: [{ id: 'BTCUSD-PERP', info: { underlyingSymbol: 'BTCUSD-INDEX' } }] },
    publicWebsocket: { isConnected: false, subscribeToMarketData: (markets) => calls.push(markets) },
  };
  const ready = venue.prepareTsReady(exchange, { isPrivate: false });
  await new Promise(setImmediate);
  assert.deepEqual(calls, []);
  exchange.publicWebsocket.isConnected = true;
  await ready;
  assert.deepEqual(calls, [[{ instrumentName: 'BTCUSD-PERP', indexInstrumentName: 'BTCUSD-INDEX' }]]);
  exchange.publicWebsocket.isConnected = false;
  await venue.prepareTsReady(exchange, { isPrivate: true });
  assert.equal(calls.length, 1);
});
