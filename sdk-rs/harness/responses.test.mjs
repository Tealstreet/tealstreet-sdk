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
