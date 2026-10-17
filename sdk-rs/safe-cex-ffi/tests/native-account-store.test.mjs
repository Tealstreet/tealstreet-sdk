import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { openStartedAccount } from './support/native-account.mjs';

test('Store notifications have one worker delivery channel', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t);
  await h.call('fetchAndSetOrders');
  await delay(30);
  assert.equal(h.events.some(event => event.event === 'update'), false, 'the TS worker does not forward full Store update events');
});

test('startup posts a complete worker Store snapshot after the loaded flags settle', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t);
  await delay(30);
  const snapshot = h.stores.at(-1);
  assert.equal(snapshot.markets?.[0]?.symbol, 'BTCUSDT');
  assert.equal(snapshot.tickers?.[0]?.last, 10);
  for (const key of ['orders', 'positions', 'displayedPositions', 'activeChasers', 'activeTWAPs']) {
    assert.ok(Array.isArray(snapshot[key]), key);
  }
  assert.equal(typeof snapshot.socketHealth?.publicHealthy, 'boolean');
  assert.equal(typeof snapshot.messageHandlerStats, 'object');
  assert.equal(snapshot.loaded.markets, true);
  assert.equal(snapshot.loaded.tickers, true);
  assert.equal(snapshot.loaded.orders, true);
});

for (const hostedMode of [false, true]) {
  test(`worker Store delivery strips raw order info and omits stable snapshots (hostedMode=${hostedMode})`, { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t, { hostedMode });
  await h.call('fetchAndSetOrders');
  await delay(350);
  const snapshot = h.stores.findLast(store => store.orders?.some(order => order.id === '77'));
  assert.ok(snapshot, 'real native order read reaches the Store callback');
  assert.equal(Object.hasOwn(snapshot.orders[0], 'info'), false);
  assert.equal(Object.hasOwn(snapshot, 'markets'), false);
  if (hostedMode) assert.equal(snapshot.tickers[0].symbol, 'BTCUSDT', 'hosted updates retain full tickers');
  else assert.equal(Object.hasOwn(snapshot, 'tickers'), false, 'unchanged browser tickers are omitted');
  });
}

for (const hostedMode of [false, true]) {
  test(`ticker bursts use the worker leading/trailing throttle (hostedMode=${hostedMode})`, { timeout: 10000 }, async t => {
    const h = await openStartedAccount(t, { hostedMode, tuning: { dataUpdateThrottleMultiplier: 2 } });
    await delay(30);
    const initial = h.stores.length;
    for (const last of [11, 12, 13, 14, 15, 16]) {
      h.sendGateway({ type: 'data', exchangeId: 'binance', channel: 'tickers', data: [{
        id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 9.9, ask: 10.1, last, mark: 10, index: 10,
        percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 1, quoteVolume: 10,
      }] });
    }
    await delay(120);
    assert.equal(h.stores.length - initial, 1, 'only the leading Store post crosses the boundary immediately');
    await delay(600);
    assert.equal(h.stores.length - initial, 2, 'one trailing post carries the latest state');
    const latest = h.stores.at(-1);
    assert.equal(Object.hasOwn(latest, 'markets'), false);
    assert.equal(Object.hasOwn(latest, 'orders'), false);
    assert.equal(Object.hasOwn(latest, 'positions'), false);
    assert.equal(latest.tickers[0].last, 16, 'gateway snapshots replace the array and advance the worker ticker version');
  });
}

test('closing an account cancels its pending trailing Store post', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t, { tuning: { dataUpdateThrottleMultiplier: 2 } });
  await h.call('fetchAndSetOrders');
  await h.call('removeOrders', [{ id: '77' }]);
  await h.close();
  await delay(30);
  const count = h.stores.length;
  await delay(600);
  assert.equal(h.stores.length, count);
});
