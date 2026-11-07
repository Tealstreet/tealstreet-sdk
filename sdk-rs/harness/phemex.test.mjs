import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { venueOf } from './venues.mjs';
const venue = venueOf('phemex');
test('Phemex shares one endpoint while keeping authenticated and public channels distinct', () => {
  const publicSocket = {}, privateSocket = {};
  venue.onSocketSend(privateSocket, JSON.stringify({ method: 'user.auth' }));
  const account = JSON.stringify({ accounts_p: [] });
  const ticker = JSON.stringify({ method: 'perp_market24h_pack_p.update', data: [] });
  assert.equal(venue.socketMatches(publicSocket, ticker), true);
  assert.equal(venue.socketMatches(privateSocket, ticker), false);
  assert.equal(venue.socketMatches(publicSocket, account), false);
  assert.equal(venue.socketMatches(privateSocket, account), true);
  assert.equal(venue.bootstrapFrame({ type: 'ws', text: '{"type":"snapshot","orders_p":[]}' }), false);
});
test('Phemex REST signature covers the decoded query and exact body', () => {
  const expiry = String(Math.round(Date.now() / 1000 + 60));
  const path = '/g-orders/create', query = 'symbol=BTCUSDT&clOrdID=caller%2Fone%20two', body = '{"x":1}';
  const signature = createHmac('sha256', 'replay-secret').update(path + decodeURIComponent(query) + expiry + body).digest('hex');
  const request = { url: `https://api.phemex.com${path}?${query}`, body, headers: { 'X-PHEMEX-ACCESS-TOKEN': 'replay-key', 'X-PHEMEX-REQUEST-EXPIRY': expiry, 'X-PHEMEX-REQUEST-SIGNATURE': signature } };
  assert.doesNotThrow(() => venue.validateRequest(request));
  assert.throws(() => venue.validateRequest({ ...request, body: '{"x":2}' }), /invalid Phemex request/);
  assert.throws(() => venue.validateRequest({ ...request, url: request.url.replace('BTCUSDT', 'ETHUSDT') }), /invalid Phemex request/);
});
test('Phemex socket audit rejects private subscriptions before a valid authentication acknowledgement', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  audit.sent('wss://ws.phemex.com', '{"method":"aop_p.subscribe","params":[]}');
  assert.equal(audit.errors().length, 1);
  assert.equal(audit.ready(), false);
});

test('Phemex full-depth audit rejects a default shallow subscription', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate:false, books:['BTCUSDT'] });
  audit.sent('wss://ws.phemex.com', '{"method":"orderbook_p.subscribe","params":["BTCUSDT"]}');
  assert.match(audit.errors().join(), /full depth0/);
});
test('Phemex history reads require authenticated headers', () => {
  for (const path of ['/api-data/g-futures/trades', '/api-data/g-futures/orders', '/api-data/g-futures/orders/by-order-id', '/api-data/futures/v2/tradeAccountDetail']) assert.throws(() => venue.validateRequest({ url:'https://api.phemex.com' + path, headers:{} }), /invalid Phemex request/);
});
