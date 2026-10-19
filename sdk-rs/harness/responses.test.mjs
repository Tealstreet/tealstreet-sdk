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
