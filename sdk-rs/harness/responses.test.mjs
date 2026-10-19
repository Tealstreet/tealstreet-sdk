import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Responses } from './responses.mjs';
import { venueOf } from './venues.mjs';

test('Bybit duplicate writes fail, unused writes count, and reads remain reusable', () => {
  const make = (method, url) => ({ type: 'http', method, url, status: 200, body: 'ok' });
  const write = 'https://api.bybit.com/v5/order/create-batch';
  const read = 'https://api.bybit.com/v5/order/realtime';
  const responses = new Responses(
    [make('POST', write), make('GET', read)],
    (method, url) => method + url,
    undefined,
    venueOf('bybit').isWrite,
  );
  assert.equal(responses.unusedWrites(), 1);
  assert.equal(responses.respond('POST', write).status, 200);
  assert.equal(responses.unusedWrites(), 0);
  assert.equal(responses.respond('POST', write).status, 599);
  assert.equal(responses.unserved, 1);
  assert.equal(responses.respond('GET', read).status, 200);
  assert.equal(responses.respond('GET', read).status, 200);
});

test('snapshot stages and headers become visible only after their command', () => {
  const read = { type: 'http', method: 'GET', url: '/balance', status: 200 };
  const write = { type: 'http', method: 'POST', url: '/change', status: 200, body: 'done', afterCommand: 2 };
  const responses = new Responses(
    [
      { ...read, body: 'old', headers: 'x-stage: 0' },
      { ...read, body: 'new', headers: 'x-stage: 2', afterCommand: 2 },
      write,
    ],
    (method, url) => method + url,
    undefined,
    (method) => method === 'POST',
  );
  assert.equal(responses.respond('GET', '/balance', undefined, 1).body, 'old');
  assert.equal(responses.respond('POST', '/change', undefined, 1).status, 599);
  assert.equal(responses.unusedWrites(), 1);
  assert.deepEqual(responses.respond('GET', '/balance', undefined, 2), {
    status: 200,
    headers: 'x-stage: 2',
    body: 'new',
  });
  assert.equal(responses.respond('POST', '/change', undefined, 2).status, 200);
  assert.equal(responses.unusedWrites(), 0);
});

test('elapsed-time snapshots retain the latest eligible read without advancing early', () => {
  const read = { type: 'http', method: 'GET', url: '/orders', status: 200 };
  const responses = new Responses([
    { ...read, body: 'initial', after: 0 },
    { ...read, body: 'updated', after: 100 },
    { ...read, body: 'command-updated', after: 150, afterCommand: 2 },
  ], (method, url) => method + url, undefined, (method) => method === 'POST');
  assert.equal(responses.respond('GET', '/orders', undefined, 0, 99).body, 'initial');
  assert.equal(responses.respond('GET', '/orders', undefined, 0, 100).body, 'updated');
  assert.equal(responses.respond('GET', '/orders', undefined, 0, 200).body, 'updated');
  assert.equal(responses.respond('GET', '/orders', undefined, 2, 149).body, 'updated');
  assert.equal(responses.respond('GET', '/orders', undefined, 2, 150).body, 'command-updated');
  assert.equal(responses.respond('GET', '/orders', undefined, 2, 200).body, 'command-updated');
});

test('POST reads stay reusable while timed mutations are consumed exactly once', () => {
  const read = { type: 'http', method: 'POST', url: '/private/get-orders', status: 200, body: 'snapshot' };
  const write = { ...read, url: '/private/create-order', body: 'created', after: 100 };
  const responses = new Responses([read, write], (method, url) => method + url, undefined,
    (method, url) => method === 'POST' && url.includes('/create-'));
  assert.equal(responses.unusedWrites(), 1);
  assert.equal(responses.respond('POST', read.url).body, 'snapshot');
  assert.equal(responses.respond('POST', read.url).body, 'snapshot');
  assert.equal(responses.respond('POST', write.url, undefined, 0, 99).status, 599);
  assert.equal(responses.respond('POST', write.url, undefined, 0, 100).body, 'created');
  assert.equal(responses.respond('POST', write.url, undefined, 0, 101).status, 599);
  assert.equal(responses.unusedWrites(), 0);
});

test('legacy body fallback cannot bypass request validation', () => {
  const records = [{ type: 'http', method: 'POST', url: '/legacy', status: 200, body: 'ok' }];
  const key = (method, url, body) => `${method} ${url} ${body ?? ''}`;
  const responses = new Responses(records, key, (_method, _url, body) => body !== 'invalid',
    () => false, (method, url) => key(method, url));
  assert.equal(responses.respond('POST', '/legacy', 'different body').body, 'ok');
  assert.equal(responses.respond('POST', '/legacy', 'invalid').status, 599);
});

test('effectiveAt reads never expose future snapshots and remain reusable at their stage', () => {
  const read = { type: 'http', method: 'POST', url: '/income', status: 200 };
  const responses = new Responses([
    { ...read, body: 'first', effectiveAt: 100 },
    { ...read, body: 'next', effectiveAt: 200 },
    { ...read, body: 'command', effectiveAt: 250, afterCommand: 2 },
  ], (method, url) => method + url, undefined, () => false);
  assert.equal(responses.respond('POST', '/income', undefined, 0, 99).status, 599);
  assert.equal(responses.respond('POST', '/income', undefined, 0, 100).body, 'first');
  assert.equal(responses.respond('POST', '/income', undefined, 0, 199).body, 'first');
  assert.equal(responses.respond('POST', '/income', undefined, 0, 200).body, 'next');
  assert.equal(responses.respond('POST', '/income', undefined, 1, 300).body, 'next');
  assert.equal(responses.respond('POST', '/income', undefined, 2, 249).body, 'next');
  assert.equal(responses.respond('POST', '/income', undefined, 2, 250).body, 'command');
  assert.equal(responses.respond('POST', '/income', undefined, 2, 251).body, 'command');
});

test('timed writes consume in tape order and generated response IDs reflect each request', () => {
  const write = { type: 'http', method: 'POST', url: '/algo', status: 200, effectiveAt: 0 };
  const responses = new Responses([
    { ...write, body: '{"orderId":"1","clientOrderId":"$clientAlgoId"}' },
    { ...write, body: '{"orderId":"2","clientOrderId":"$newClientOrderId"}' },
  ], (method, url) => method + url);
  assert.equal(responses.unusedWrites(), 2);
  assert.deepEqual(JSON.parse(responses.respond('POST', '/algo', '{"clientAlgoId":"a\\\"b"}').body),
    { orderId: '1', clientOrderId: 'a"b' });
  assert.deepEqual(JSON.parse(responses.respond('POST', '/algo', { newClientOrderId: 'c' }).body),
    { orderId: '2', clientOrderId: 'c' });
  assert.equal(responses.respond('POST', '/algo').status, 599);
  assert.equal(responses.unusedWrites(), 0);
});

test('default BingX writes are consumed once and listen-key lifecycle stays reusable', () => {
  const order = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';
  const listen = 'https://open-api.bingx.com/openApi/user/auth/userDataStream';
  const binanceListen = 'https://fapi.binance.com/fapi/v1/listenKey';
  const records = [order, listen, binanceListen].flatMap((url) =>
    ['POST', 'PUT', 'DELETE'].map((method) => ({ type: 'http', method, url, status: 200, body: 'ok' })),
  );
  const responses = new Responses(records, (method, url) => method + url);
  assert.equal(responses.unusedWrites(), 3);
  for (const method of ['POST', 'PUT', 'DELETE']) {
    assert.equal(responses.respond(method, order).status, 200);
    assert.equal(responses.respond(method, order).status, 599);
    for (const url of [listen, binanceListen]) {
      assert.equal(responses.respond(method, url).status, 200);
      assert.equal(responses.respond(method, url).status, 200);
    }
  }
  assert.equal(responses.unusedWrites(), 0);
});

test('time and command boundaries both guard reusable snapshots; failed checks do not consume writes', () => {
  const url = 'https://example.test/state';
  const records = [
    { type: 'http', method: 'GET', url, status: 200, body: 'old' },
    { type: 'http', method: 'GET', url, status: 200, body: 'new', afterCommand: 2, availableAfter: 100 },
    { type: 'http', method: 'POST', url, status: 200, body: 'written' },
  ];
  const responses = new Responses(
    records,
    (method, url) => method + url,
    (_method, _url, body) => body !== 'invalid',
  );
  assert.equal(responses.respond('GET', url, undefined, 1, 100).body, 'old');
  assert.equal(responses.respond('GET', url, undefined, 2, 99).body, 'old');
  assert.equal(responses.respond('GET', url, undefined, 2, 100).body, 'new');
  assert.equal(responses.respond('GET', url, undefined, 2, 100).body, 'new');
  assert.equal(responses.respond('POST', url, 'invalid').status, 599);
  assert.equal(responses.unusedWrites(), 1);
  assert.equal(responses.respond('POST', url).body, 'written');
});

test('Crypto authenticated POST reads reuse snapshots while writes remain single-use', () => {
  const read = 'https://api.crypto.com/exchange/v1/private/get-open-orders';
  const write = 'https://api.crypto.com/exchange/v1/private/create-order';
  const responses = new Responses(
    [read, write].map((url) => ({ type: 'http', method: 'POST', url, status: 200, body: 'ok' })),
    (method, url) => method + url,
    undefined,
    venueOf('cryptocom').isWrite,
  );
  assert.equal(responses.unusedWrites(), 1);
  assert.equal(responses.respond('POST', read).status, 200);
  assert.equal(responses.respond('POST', read).status, 200);
  assert.equal(responses.respond('POST', write).status, 200);
  assert.equal(responses.respond('POST', write).status, 599);
  assert.equal(responses.unusedWrites(), 0);
});

test('effectiveAt aliases availableAfter and never serves a future-only snapshot', () => {
  const url = 'https://example.test/state';
  const responses = new Responses(
    [{ type: 'http', method: 'GET', url, status: 200, body: 'future', effectiveAt: 100 }],
    (method, url) => method + url,
  );
  assert.equal(responses.respond('GET', url, undefined, 0, 99).status, 599);
  assert.equal(responses.respond('GET', url, undefined, 0, 100).body, 'future');
  assert.equal(responses.respond('GET', url, undefined, 0, 100).body, 'future');
});
