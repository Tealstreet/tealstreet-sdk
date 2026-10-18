import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
async function open(t, events = []) {
  const id = await native.nativeAccountOpen({
    exchange: 'binance', options: { key: 'availability-key', secret: '' },
  }, event => events.push(event), () => {}, async () => { throw Error('unexpected signing request'); });
  t.after(() => native.nativeAccountClose(id));
  return id;
}

async function eventually(predicate) {
  for (let i = 0; i < 100; i++) {
    if (predicate()) return;
    await delay(10);
  }
  assert.fail('native signer-availability event did not arrive');
}

test('acknowledged signer updates are visible before the following operation', { timeout: 3000 }, async t => {
  const events = [];
  const id = await open(t, events);
  const acknowledgement = native.nativeAccountSignerAvailable(id, false);
  assert.ok(acknowledgement instanceof Promise, 'valid availability changes return an acknowledgement Promise');
  assert.equal(await acknowledgement, undefined);
  let settled = false;
  const waiting = native.nativeAccountCallMethod(id, 'waitForRemoteSigner', []).then(result => {
    settled = true;
    return result;
  });
  await eventually(() => events.some(event => event.event === 'remoteSignerStatus' && event.data === 'disconnected'));
  assert.equal(settled, false, 'the following operation observes unavailable state');
  await native.nativeAccountSignerAvailable(id, true);
  assert.equal(await waiting, true, 'acknowledged availability releases the actual signer waiter');
  await eventually(() => events.some(event => event.event === 'remoteSignerStatus' && event.data === 'connected'));
});

test('signer updates on released handles reject with the typed account-closed error', async t => {
  const id = await open(t);
  await native.nativeAccountClose(id);
  await assert.rejects(async () => native.nativeAccountSignerAvailable(id, false), error => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'AccountClosedError');
    assert.equal(error.code, 'ACCOUNT_CLOSED');
    assert.equal(error.message, `native account ${id} is closed`);
    return true;
  });
});

for (const id of [0, 0xffffffff]) test(`signer updates on never-issued handle ${id} reject visibly`, async () => {
  await assert.rejects(async () => native.nativeAccountSignerAvailable(id, true), error => {
    assert.equal(error.name, 'Error');
    assert.equal(error.message, `no native account ${id}`);
    assert.equal(Object.hasOwn(error, 'code'), false);
    return true;
  });
});

test('dispose retains the handle for acknowledged signer updates', async t => {
  const id = await open(t);
  await native.nativeAccountCallMethod(id, 'dispose', []);
  assert.equal(await native.nativeAccountSignerAvailable(id, false), undefined);
  assert.equal(await native.nativeAccountCallMethod(id, 'waitForRemoteSigner', []), false);
});
