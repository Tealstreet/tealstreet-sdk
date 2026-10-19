import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Responses } from './responses.mjs';
import { venueOf } from './venues.mjs';

test('Bybit duplicate writes fail, unused writes count, and reads remain reusable', () => {
  const make = (method, url) => ({ type: 'http', method, url, status: 200, body: 'ok' });
  const write = 'https://api.bybit.com/v5/order/create-batch';
  const read = 'https://api.bybit.com/v5/order/realtime';
  const responses = new Responses([make('POST', write), make('GET', read)], (method, url) => method + url, undefined, venueOf('bybit').isWrite);
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
  const responses = new Responses([
    { ...read, body: 'old', headers: 'x-stage: 0' },
    { ...read, body: 'new', headers: 'x-stage: 2', afterCommand: 2 }, write,
  ], (method, url) => method + url, undefined, (method) => method === 'POST');
  assert.equal(responses.respond('GET', '/balance', undefined, 1).body, 'old');
  assert.equal(responses.respond('POST', '/change', undefined, 1).status, 599);
  assert.equal(responses.unusedWrites(), 1);
  assert.deepEqual(responses.respond('GET', '/balance', undefined, 2), { status: 200, headers: 'x-stage: 2', body: 'new' });
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
