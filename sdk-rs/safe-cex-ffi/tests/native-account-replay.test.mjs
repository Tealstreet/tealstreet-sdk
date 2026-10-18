import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
const native = createRequire(import.meta.url)(process.env.LIVE_DIFF_ADDON ?? '../dist/node/safe_cex_ffi.node');
const init = { exchange: 'binance', options: { key: '', secret: '', publicDataOnly: true } };

test('compiled addon isolates replay clock and exposes processed-frame receipts', { timeout: 10000 }, async (t) => {
  const ordinary = await native.nativeAccountOpen(init, () => {}, () => {});
  t.after(() => native.nativeAccountClose(ordinary));
  await assert.rejects(native.nativeAccountReplay(ordinary, { op: 'receipt' }), /no replay host/);
  await assert.rejects(native.nativeAccountOpen({ ...init, replayEpochMs: 1800000000000 }, () => {}, () => {}), /isolated native host/);
  await native.nativeAccountClose(ordinary);
  const id = await native.nativeAccountOpen({ ...init, replayEpochMs: 1800000000000 }, () => {}, () => {});
  try {
    await assert.rejects(native.nativeAccountReplay(id, { op: 'advance', atMs: 1 }), /active/);
    const began = await native.nativeAccountReplay(id, { op: 'begin' });
    assert.equal(began.atMs, 0);
    assert.deepEqual(began.sockets, []);
    assert.deepEqual(began.transport, []);
    await assert.rejects(native.nativeAccountReplay(id, { op: 'begin' }), /already began/);
    const advanced = await native.nativeAccountReplay(id, { op: 'advance', atMs: 250.75 });
    assert.equal(advanced.atMs, 250.75);
    await assert.rejects(native.nativeAccountReplay(id, { op: 'advance', atMs: 200 }), /monotonic/);
    await assert.rejects(native.nativeAccountReplay(id, { op: 'advance', atMs: NaN }), /finite/);
    assert.equal((await native.nativeAccountReplay(id, { op: 'receipt' })).atMs, 250.75);
  } finally { await native.nativeAccountClose(id); }
  await assert.rejects(native.nativeAccountReplay(id, { op: 'receipt' }), /closed/);
  await assert.rejects(native.nativeAccountOpen(init, () => {}, () => {}), /isolated native host/);
});
