import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { openStartedAccount, orderOptions } from './support/native-account.mjs';

test('worker error events retain the real Binance preflight metadata argument', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t);
  await assert.rejects(h.call('placeOrderWithClientOrderId', orderOptions), /requires a clientOrderId/);
  for (let i = 0; i < 100 && !h.events.some(event => event.event === 'error'); i++) await delay(10);
  const event = h.events.find(event => event.event === 'error');
  assert.ok(event, 'the real registry preflight emitted its error');
  assert.ok(Array.isArray(event.data), `worker rest arguments expected: ${JSON.stringify(event)}`);
  assert.equal(event.data.length, 2);
  assert.match(event.data[0], /binance placeOrderWithClientOrderId requires a clientOrderId/);
  assert.equal(event.data[1].suppressConsoleLog, false);
  assert.equal(h.seen.length, 0, 'the actual venue rejected placement before HTTP');
});
