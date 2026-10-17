import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { openStartedAccount } from './support/native-account.mjs';

async function eventually(predicate) {
  for (let i = 0; i < 200; i++) { if (predicate()) return; await delay(10); }
  assert.fail('native gateway/stream callback did not arrive');
}
for (const [method, args, channel, data] of [
  ['listenOHLCV', [{ symbol: 'BTCUSDT', interval: '1m' }], 'candle',
    { timestamp: 1700000000000, open: 9, high: 12, low: 8, close: 11, volume: 17 }],
  ['listenOrderBook', ['BTCUSDT'], 'orderbook',
    { bids: [{ price: 9.9, amount: 2, total: 2 }], asks: [{ price: 10.1, amount: 3, total: 3 }] }],
]) {
  test(`${method} forwards live venue callbacks and cleans up the remote subscription`, { timeout: 10000 }, async t => {
    const h = await openStartedAccount(t);
    const subscriptionId = await h.call(method, ...args);
    assert.equal(typeof subscriptionId, 'number');
    await eventually(() => h.received.some(frame => frame.type === 'subscribe' && frame.channel === channel));
    const frame = { type: 'data', exchangeId: 'binance', channel, symbol: 'BTCUSDT', data };
    if (channel === 'candle') frame.interval = '1m';
    h.sendGateway(frame);
    await eventually(() => h.events.some(event => event.event === method));
    assert.deepEqual(h.events.find(event => event.event === method).data, { subscriptionId, data });
    assert.equal(await h.call('unlisten', subscriptionId), null);
    assert.equal(await h.call('unlisten', subscriptionId), null, 'cleanup is idempotent');
    await eventually(() => h.received.some(frame => frame.type === 'unsubscribe' && frame.channel === channel));
    const count = h.events.filter(event => event.event === method).length;
    h.sendGateway(frame);
    await delay(100);
    assert.equal(h.events.filter(event => event.event === method).length, count);
  });
}

test('listenOHLCVs reaches the TS base no-op on Binance', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t);
  const subscriptionId = await h.call('listenOHLCVs', { symbols: ['BTCUSDT'], interval: '1m' });
  assert.equal(typeof subscriptionId, 'number');
  await h.call('unlisten', subscriptionId);
  assert.ok(h.received.every(frame => frame.channel !== 'candle'));
  assert.equal(h.seen.length, 0);
});

test('dispose cancels live stream registrations', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t);
  const subscriptionId = await h.call('listenOHLCV', { symbol: 'BTCUSDT', interval: '1m' });
  await eventually(() => h.received.some(frame => frame.type === 'subscribe' && frame.channel === 'candle'));
  assert.equal(await h.call('dispose'), null);
  assert.equal(await h.call('unlisten', subscriptionId), null, 'dispose already released the handle');
  const count = h.events.filter(event => event.event === 'listenOHLCV').length;
  h.sendGateway({ type: 'data', exchangeId: 'binance', channel: 'candle', symbol: 'BTCUSDT', interval: '1m',
    data: { timestamp: 1700000000000, open: 9, high: 12, low: 8, close: 11, volume: 17 } });
  await delay(100);
  assert.equal(h.events.filter(event => event.event === 'listenOHLCV').length, count);
});
