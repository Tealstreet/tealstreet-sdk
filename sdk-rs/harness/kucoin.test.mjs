import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { kucoin } from './kucoin.mjs';
const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('base64');
const sign = (method, path, body = '') => {
  const timestamp = String(Date.now());
  return { 'KC-API-KEY': 'replay-key', 'KC-API-TIMESTAMP': timestamp, 'KC-API-KEY-VERSION': '2', 'KC-API-PASSPHRASE': hmac('replay-passphrase'), 'KC-API-SIGN': hmac(timestamp + method + path + body) };
};
test('KuCoin signature audit binds raw query, body, timestamp and signed passphrase', () => {
  const path = '/api/v1/orders?clientOid=a b', url = 'https://api-futures.kucoin.com/api/v1/orders?clientOid=a%20b';
  const headers = sign('GET', path);
  assert.equal(kucoin.checkRequest('GET', url, undefined, headers), true);
  assert.equal(kucoin.checkRequest('DELETE', url, undefined, headers), false);
  assert.equal(kucoin.checkRequest('GET', url, undefined, { ...headers, 'KC-API-PASSPHRASE': 'replay-passphrase' }), false);
  assert.equal(kucoin.checkRequest('GET', url.replace('a%20b', 'ab'), undefined, headers), false);
  const body = '{"size":"2"}', post = sign('POST', '/api/v1/orders', body);
  assert.equal(kucoin.checkRequest('POST', 'https://api-futures.kucoin.com/api/v1/orders', body, post), true);
  assert.equal(kucoin.checkRequest('POST', 'https://api-futures.kucoin.com/api/v1/orders', '{"size":"3"}', post), false);
  assert.equal(kucoin.checkRequest('POST', 'https://api-futures.kucoin.com/api/v1/bullet-private', undefined, {}), false);
  assert.equal(kucoin.isWrite('POST', 'https://api-futures.kucoin.com/api/v1/bullet-private'), false);
});
const endpoint = 'wss://example.test/socket';
const records = ['public', 'private'].map((kind) => ({ type: 'http', url: `https://api-futures.kucoin.com/api/v1/bullet-${kind}`, body: JSON.stringify({ code: '200000', data: { token: kind + '-token', instanceServers: [{ endpoint }] } }) }));
test('KuCoin token audit validates endpoint, private topics and no follow-up login', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: true, books: [], records });
  const url = endpoint + '?token=private-token&connectId=123';
  for (const topic of ['/contractMarket/tradeOrders', '/contractMarket/advancedOrders', '/contract/positionAll', '/contractAccount/wallet', '/contract/marginMode', '/contract/crossLeverage']) audit.sent(url, JSON.stringify({ type: 'subscribe', topic, privateChannel: true }));
  assert.equal(audit.ready(), true);
  assert.deepEqual(audit.errors(), []);
  audit.sent(url, '{"type":"login"}');
  assert(audit.errors().some((e) => e.includes('authentication')));
});
test('KuCoin rejects wrong token, unsubscribed frames and session topic overflow', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: false, books: [], records });
  const url = endpoint + '?token=public-token&connectId=123';
  audit.received(url, '{"type":"message","topic":"/contractMarket/level2:XBTUSDTM"}');
  for (let i = 0; i < 5; i++) audit.sent(url, JSON.stringify({ type: 'subscribe', topic: '/contractMarket/snapshot:' + Array.from({ length: 100 }, (_, j) => `S${i * 100 + j}`).join(','), privateChannel: false }));
  audit.sent(endpoint + '?token=bad&connectId=123', '{"type":"ping","id":1}');
  assert(audit.errors().some((e) => e.includes('unsubscribed')));
  assert(audit.errors().some((e) => e.includes('400')));
  assert(audit.errors().some((e) => e.includes('bullet token')));
});

test('KuCoin readiness requires active bid/ask and mark/index topics for each book', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: false, books: ['BTCUSDT'], records });
  const url = endpoint + '?token=public-token&connectId=123';
  const send = (topic) => audit.sent(url, JSON.stringify({ type: 'subscribe', topic, privateChannel: false }));
  send('/contractMarket/level2:XBTUSDTM');
  assert.equal(audit.ready(), false);
  send('/contractMarket/tickerV2:XBTUSDTM');
  assert.equal(audit.ready(), false);
  send('/contract/instrument:XBTUSDTM');
  assert.equal(audit.ready(), true);
});
