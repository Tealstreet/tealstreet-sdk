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

test('implementation-specific responses preserve exact matching and selected write consumption', () => {
  const make = (requestBody, impl) => ({
    type: 'http', method: 'POST', url: '/cancel', requestBody, status: 200, body: 'ok',
    ...(impl && { impl }),
  });
  const records = [make('shared'), make('legacy', 'ts'), make('Order', 'rust'), make('StopOrder', 'rust')];
  const key = (method, url, body) => `${method} ${url} ${body}`;
  const write = (method) => method === 'POST';
  for (const impl of ['ts', 'rust']) {
    const responses = new Responses(records, key, undefined, write, impl);
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
  assert.throws(() => new Responses(records, key, undefined, write, 'typo'), /Unknown replay implementation/);
  assert.throws(() => new Responses([make('bad', 'typo')], key, undefined, write, 'rust'), /Unknown HTTP record/);
});
