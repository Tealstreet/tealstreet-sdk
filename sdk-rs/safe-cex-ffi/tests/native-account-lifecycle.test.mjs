import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {test} from 'node:test';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const open = () => native.nativeAccountOpen({
  exchange:'binance', optionsJson:JSON.stringify({key:'lifecycle-key',secret:'lifecycle-secret'}),
}, () => {}, () => {});

test('closing an issued native account twice is an idempotent no-op', async () => {
  const id = await open();
  await native.nativeAccountClose(id);
  await assert.doesNotReject(native.nativeAccountClose(id));
});

test('method calls on released handles reject with a typed account-closed error', async () => {
  const id = await open();
  await native.nativeAccountClose(id);
  await assert.rejects(native.nativeAccountCallMethod(id, 'fetchOrders', '[]'), error => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'AccountClosedError');
    assert.equal(error.code, 'ACCOUNT_CLOSED');
    assert.equal(error.message, `native account ${id} is closed`);
    return true;
  });
});

test('callMethod dispose retains the issued handle for source-compatible calls', async () => {
  const id = await open();
  try {
    await native.nativeAccountCallMethod(id, 'dispose', '[]');
    await native.nativeAccountCallMethod(id, 'dispose', '[]');
    assert.deepEqual(JSON.parse(await native.nativeAccountCallMethod(id, 'getActiveTickerSymbols', '[]')), []);
  } finally {
    await native.nativeAccountClose(id);
  }
});
