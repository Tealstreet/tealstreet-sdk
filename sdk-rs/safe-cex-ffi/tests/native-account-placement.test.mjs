import assert from 'node:assert/strict';
import { test } from 'node:test';
import { openStartedAccount, orderOptions } from './support/native-account.mjs';

test('mintClientOrderId produces an honorable Binance broker ID', { timeout: 10000 }, async t => {
  const { call } = await openStartedAccount(t);
  const id = await call('mintClientOrderId', orderOptions);
  assert.equal(typeof id, 'string');
  assert.match(id, /^x-s3vFFRQp[A-Za-z0-9]{1,21}$/);
});

test('placeOrderWithClientOrderId sends the exact ID and returns the TS reference', { timeout: 10000 }, async t => {
  const { call, seen } = await openStartedAccount(t);
  assert.deepEqual(await call('placeOrderWithClientOrderId', { ...orderOptions, clientOrderId: 'caller-native-77' }),
    { orderIds: ['9001'], clientOrderId: 'caller-native-77' });
  const writes = seen.filter(request => request.method === 'POST');
  assert.equal(writes.length, 1);
  assert.equal(writes[0].params.get('newClientOrderId'), 'caller-native-77');
  await assert.rejects(call('placeOrderWithClientOrderId', orderOptions), /requires a clientOrderId/);
  assert.equal(seen.filter(request => request.method === 'POST').length, 1, 'missing caller ID rejects before a write');
});

test('existing placeOrder uses the TS filtered-nanoid format', { timeout: 10000 }, async t => {
  const { call, seen } = await openStartedAccount(t);
  assert.deepEqual(await call('placeOrder', orderOptions), ['9001']);
  const write = seen.find(request => request.method === 'POST');
  assert.match(write.params.get('newClientOrderId'), /^x-s3vFFRQp[A-Za-z0-9]{1,21}$/);
});

for (const [method, args, message] of [
  ['fetchOptionChain', [{ underlying: 'BTC' }], 'Option chains are not supported on binance'],
  ['fetchOptionExpiries', ['BTC'], 'Option expiries are not supported on binance'],
  ['fetchAccountGreeks', [], 'Account greeks are not supported on binance'],
]) {
  test(`${method} reaches the venue capability`, { timeout: 10000 }, async t => {
    const { call } = await openStartedAccount(t);
    await assert.rejects(call(method, ...args), error => error.message.includes(message));
  });
}
for (const method of ['nuke', 'nukeOrders']) {
  test(`${method} cancels the account's native orders`, { timeout: 10000 }, async t => {
    const { call, seen } = await openStartedAccount(t);
    await call('fetchAndSetOrders', null);
    assert.equal(await call(method), null);
    assert.deepEqual(seen.filter(request => request.method === 'DELETE').map(request => request.path).sort(),
      ['/fapi/v1/algoOpenOrders', '/fapi/v1/allOpenOrders']);
  });
}
test('nukePositions leaves native orders alone', { timeout: 10000 }, async t => {
  const { call, seen } = await openStartedAccount(t);
  await call('fetchAndSetOrders', null);
  assert.equal(await call('nukePositions'), null);
  assert.ok(seen.every(request => request.method === 'GET'));
});
