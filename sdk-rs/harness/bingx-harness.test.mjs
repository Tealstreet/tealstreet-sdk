import assert from 'node:assert/strict';
import test from 'node:test';
import { requestKey, selectResponse, subscriptionKeys } from './requests.mjs';
import { venueOf } from './venues.mjs';
const url = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';

test('BingX request matching retains economic wire types and exact caller IDs', () => {
  const ids = new Set(['caller']);
  const key = (body) => requestKey('POST', url, body, ids);
  assert.notEqual(key({ quantity: 1 }), key({ quantity: '1' }));
  assert.notEqual(key({ takeProfit: { stopPrice: 1 } }), key({ takeProfit: '{"stopPrice":1}' }));
  assert.notEqual(key({ clientOrderId: 'caller' }), key({ clientOrderId: 'generated' }));
  assert.equal(key({ clientOrderId: 'generated-a', timestamp: 1 }), key({ clientOrderId: 'generated-b', timestamp: 2 }));
  assert.equal(key({ stop: { price: 1, quantity: 2 } }), key({ stop: { quantity: 2, price: 1 } }));
  assert.notEqual(key('{"cancelOrderId":8389765519993909000}'), key('{"cancelOrderId":8389765519993909001}'));
  assert.notEqual(key('{"cancelOrderId":8389765519993909000}'), key('{"cancelOrderId":"8389765519993909000"}'));
  assert.notEqual(key('[1]'), key('{"0":1}'));
});

test('BingX native subscriptions retain channel, operation, socket and reconnect generation', () => {
  const options = venueOf('bingx');
  const socket = 'wss://ws-market-swap.qq-os.com/ws';
  const keys = (reqType, dataType = 'market.contracts', id = 'one') => subscriptionKeys(socket, { reqType, dataType, id }, new Set(), options);
  assert.deepEqual(keys('sub'), keys('sub', 'market.contracts', 'another'));
  assert.notDeepEqual(keys('sub'), keys('unsub'));
  assert.notDeepEqual(keys('sub'), keys('sub', 'market.depth.BTC-USDT.step0'));
  assert.deepEqual(subscriptionKeys(socket, 'Ping', new Set(), options), []);
  assert.deepEqual(subscriptionKeys(socket, '{"pong":1}', new Set(), options), []);
  const first = keys('sub').map((key) => `1 ${key}`), second = keys('sub').map((key) => `2 ${key}`);
  assert.notDeepEqual(first, second);
  assert.equal(options.subscriptionGenerations, true);
});

test('BingX availableAfter observes both command and delivered time, without reusing writes', () => {
  const initial = { body: 'old' }, next = { body: 'new', availableAfter: 100, afterCommand: 2 };
  const queue = [initial, next];
  assert.equal(selectResponse(queue, 100, { commandIndex: 1 }), initial);
  assert.equal(selectResponse(queue, 99, { commandIndex: 2 }), initial);
  assert.equal(selectResponse(queue, 100, { commandIndex: 2 }), next);
  assert.equal(selectResponse(queue, 100, { commandIndex: 2 }), next);
  const writes = [{ body: 'first', afterCommand: 2 }, { body: 'second' }];
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 1 }), undefined);
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 2 }).body, 'first');
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 2 }).body, 'second');
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 2 }), undefined);
});
