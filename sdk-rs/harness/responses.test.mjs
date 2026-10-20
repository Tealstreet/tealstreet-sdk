import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Responses } from './responses.mjs';
import { venueOf } from './venues.mjs';

test('concurrent replies replay in request order while legacy tapes retain completion order', () => {
  const reply = (body, ordinal) => ({ type: 'http', method: 'GET', url: '/token', status: 200, body,
    ...(ordinal === undefined ? {} : { requestOrdinal: ordinal }) });
  const key = (method, url) => method + url;
  const ordered = new Responses([reply('second', 7), reply('first', 3)], key);
  assert.equal(ordered.respond('GET', '/token').body, 'first');
  assert.equal(ordered.respond('GET', '/token').body, 'second');
  assert.equal(ordered.respond('GET', '/token').body, 'second');
  const legacy = new Responses([reply('second'), reply('first')], key);
  assert.equal(legacy.respond('GET', '/token').body, 'second');
  assert.equal(legacy.respond('GET', '/token').body, 'first');
  for (const ordinals of [[1, 1], [1, undefined], [-1, 0], [0.5, 1], [0, Infinity]]) {
    assert.throws(() => new Responses(ordinals.map((ordinal) => reply('bad', ordinal)), key), /request ordinals/);
  }
});

test('request ordering preserves write stages and does not reorder timed snapshots', () => {
  const key = (method, url) => method + url;
  const write = (body, requestOrdinal, afterCommand = 0) => ({
    type: 'http', method: 'POST', url: 'https://example.test/order', status: 200, body, requestOrdinal, afterCommand,
  });
  const responses = new Responses([write('second', 9), write('first', 2, 1)], key);
  assert.equal(responses.respond('POST', 'https://example.test/order').status, 599);
  assert.equal(responses.unusedWrites(), 2);
  assert.equal(responses.respond('POST', 'https://example.test/order', undefined, 1).body, 'first');
  assert.equal(responses.respond('POST', 'https://example.test/order', undefined, 1).body, 'second');
  assert.equal(responses.unusedWrites(), 0);
  const timed = new Responses([
    { ...write('new', 1), method: 'GET', effectiveAt: 50 },
    { ...write('old', 2), method: 'GET', effectiveAt: 0 },
  ], key);
  assert.equal(timed.respond('GET', 'https://example.test/order', undefined, 0, 49).body, 'old');
  assert.equal(timed.respond('GET', 'https://example.test/order', undefined, 0, 50).body, 'new');
});

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


test('timed snapshots advance with delivered events inside their command stage', () => {
  const read = { type: 'http', method: 'POST', url: 'https://api.hyperliquid.xyz/info', status: 200 };
  const responses = new Responses([
    { ...read, body: 'startup' },
    { ...read, body: 'placed', afterCommand: 1, availableAt: 500 },
    { ...read, body: 'cancelled', afterCommand: 1, availableAt: 1500 },
  ], (method, url) => method + url);
  assert.equal(responses.respond(read.method, read.url, undefined, 0, 500).body, 'startup');
  assert.equal(responses.respond(read.method, read.url, undefined, 1, 499).body, 'startup');
  assert.equal(responses.respond(read.method, read.url, undefined, 1, 500).body, 'placed');
  assert.equal(responses.respond(read.method, read.url, undefined, 1, 1499).body, 'placed');
  assert.equal(responses.respond(read.method, read.url, undefined, 1, 1500).body, 'cancelled');
});

test('writes cannot skip a premature earlier acknowledgement or consume on failed validation', () => {
  const write = { type: 'http', method: 'POST', url: 'https://api.example/order', status: 200 };
  const records = [{ ...write, body: 'first', afterCommand: 2 }, { ...write, body: 'second' }];
  const responses = new Responses(records, (method, url) => method + url, (_method, _url, body) => body === 'valid');
  assert.equal(responses.respond(write.method, write.url, 'invalid', 2).status, 599);
  assert.equal(responses.unusedWrites(), 2);
  assert.equal(responses.respond(write.method, write.url, 'valid', 1).status, 599);
  assert.equal(responses.respond(write.method, write.url, 'valid', 2).body, 'first');
  assert.equal(responses.respond(write.method, write.url, 'valid', 2).body, 'second');
  assert.equal(responses.respond(write.method, write.url, 'valid', 2).status, 599);
  assert.equal(responses.unusedWrites(), 0);
});


test('WEEX effective snapshots follow delivered time without consuming their transitions', () => {
  const read = { type: 'http', method: 'GET', url: 'https://api-contract.weex.com/capi/v3/openOrders', status: 200 };
  const responses = new Responses([
    { ...read, effectiveAt: 0, body: 'initial' },
    { ...read, effectiveAt: 2800, body: 'placed' },
    { ...read, effectiveAt: 4000, body: 'cancelled' },
  ], (method, url) => method + url);
  for (let i = 0; i < 3; i++) assert.equal(responses.respond(read.method, read.url, undefined, 0, 2799).body, 'initial');
  assert.equal(responses.respond(read.method, read.url, undefined, 0, 2800).body, 'placed');
  assert.equal(responses.respond(read.method, read.url, undefined, 0, 3999).body, 'placed');
  assert.equal(responses.respond(read.method, read.url, undefined, 0, 4000).body, 'cancelled');
  const future = new Responses([{ ...read, effectiveAt: 1, body: 'future' }], (method, url) => method + url);
  assert.equal(future.respond(read.method, read.url, undefined, 0, 0).status, 599);
});

test('WEEX acknowledgements echo the actual generated ID and still consume exactly once', () => {
  const write = { type: 'http', method: 'POST', url: 'https://api-contract.weex.com/capi/v3/algoOrder', status: 200,
    body: '{"orderId":"702345678901234565","clientOrderId":"$clientAlgoId"}' };
  const responses = new Responses([write], (method, url) => method + url);
  const result = responses.respond(write.method, write.url, '{"clientAlgoId":"generated-on-wire"}');
  assert.equal(JSON.parse(result.body).clientOrderId, 'generated-on-wire');
  assert.equal(JSON.parse(result.body).orderId, '702345678901234565');
  assert.equal(responses.respond(write.method, write.url, '{"clientAlgoId":"another"}').status, 599);
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

test('response time visibility and header authentication remain independent', () => {
  const url = 'https://venue.test/private';
  const records = [{ type: 'http', method: 'GET', url, availableAfter: 100, status: 200, body: '{}' }];
  const responses = new Responses(records, () => 'read', (_method, _url, _body, headers) => headers?.auth === 'fixture', () => false);
  assert.equal(responses.respond('GET', url, undefined, 0, 99, { auth: 'fixture' }).status, 599);
  assert.equal(responses.respond('GET', url, undefined, 0, 100, {}).status, 599);
  assert.equal(responses.respond('GET', url, undefined, 0, 100, { auth: 'fixture' }).status, 200);
});

test('implementation-specific responses preserve exact matching and selected write consumption', () => {
  const make = (requestBody, impl) => ({
    type: 'http',
    method: 'POST',
    url: '/cancel',
    requestBody,
    status: 200,
    body: 'ok',
    ...(impl && { impl }),
  });
  const records = [make('shared'), make('legacy', 'ts'), make('Order', 'rust'), make('StopOrder', 'rust')];
  const key = (method, url, body) => `${method} ${url} ${body}`;
  const write = (method) => method === 'POST';
  for (const impl of ['ts', 'rust']) {
    const responses = new Responses(records, key, undefined, write, true, impl);
    assert.equal(responses.unusedWrites(), impl === 'ts' ? 2 : 3);
    assert.equal(responses.respond('POST', '/cancel', 'shared').status, 200);
    const selected = impl === 'ts' ? 'legacy' : 'Order';
    const excluded = impl === 'ts' ? 'Order' : 'legacy';
    assert.equal(responses.respond('POST', '/cancel', excluded).status, 599);
    assert.equal(responses.respond('POST', '/cancel', selected).status, 200);
    assert.equal(responses.respond('POST', '/cancel', selected).status, 599);
    assert.equal(responses.unserved, 2);
    assert.equal(responses.unusedWrites(), impl === 'ts' ? 0 : 1);
    if (impl === 'rust') {
      assert.equal(responses.respond('POST', '/cancel', 'StopOrder').status, 200);
      assert.equal(responses.unusedWrites(), 0);
    }
  }
  assert.throws(() => new Responses(records, key, undefined, write), /require a replay implementation/);
  assert.throws(() => new Responses(records, key, undefined, write, true, 'typo'), /Unknown replay implementation/);
  assert.throws(() => new Responses([make('bad', 'typo')], key, undefined, write, true, 'rust'), /Unknown HTTP record/);
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
