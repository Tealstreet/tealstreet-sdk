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
