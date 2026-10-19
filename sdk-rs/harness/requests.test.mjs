import assert from 'node:assert/strict';
import { test } from 'node:test';
import { requestKey, takeResponse } from './requests.mjs';
const url = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';

test('write matching preserves numeric/body types and exact caller IDs', () => {
  const ids = new Set(['caller-lowercase']);
  const key = (body) => requestKey('POST', url, body, ids);
  assert.notEqual(key({ quantity: 1 }), key({ quantity: '1' }));
  assert.notEqual(key({ takeProfit: { stopPrice: 1 } }), key({ takeProfit: '{"stopPrice":1}' }));
  assert.notEqual(key({ clientOrderId: 'caller-lowercase' }), key({ clientOrderId: 'generated' }));
  assert.equal(key({ clientOrderId: 'tsgenerated1', timestamp: 1, quantity: 1 }), key({ clientOrderId: 'tsgenerated2', timestamp: 2, quantity: 1 }));
});
test('a single recorded write cannot serve duplicate submissions', () => {
  const response = { status: 200, body: '{}' };
  for (const method of ['POST', 'PUT', 'DELETE']) {
    const queue = [response];
    assert.equal(takeResponse(queue, method, url), response);
    assert.equal(takeResponse(queue, method, url), undefined);
  }
});
test('polling and listen-key reads can reuse responses; snapshots can change with tape time', () => {
  const first = { body: 'first' };
  const next = { body: 'next', availableAfter: 1000 };
  const queue = [first, next];
  assert.equal(takeResponse(queue, 'GET', url, 999), first);
  assert.equal(takeResponse(queue, 'GET', url, 1000), next);
  const auth = [first];
  const path = 'https://open-api.bingx.com/openApi/user/auth/userDataStream';
  assert.equal(takeResponse(auth, 'POST', path), first);
  assert.equal(takeResponse(auth, 'PUT', path), first);
  assert.equal(auth.length, 1);
});
test('Binance query matching preserves a caller ID while ignoring generated IDs', () => {
  const root = 'https://fapi.binance.com/fapi/v1/order';
  const ids = new Set(['caller']);
  assert.equal(requestKey('POST', `${root}?newClientOrderId=x&timestamp=1`), requestKey('POST', `${root}?newClientOrderId=y&timestamp=2`));
  assert.notEqual(requestKey('POST', `${root}?newClientOrderId=caller`, undefined, ids), requestKey('POST', `${root}?newClientOrderId=changed`, undefined, ids));
});
