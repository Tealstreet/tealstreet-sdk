import { test } from 'node:test';
import assert from 'node:assert/strict';
import { retainedMemory, settledMemory } from './memory.mjs';

test('retained frame backing buffers are included', () => {
  const before = process.memoryUsage();
  const frame = new ArrayBuffer(16 * 1024 * 1024);
  const after = process.memoryUsage();
  assert(retainedMemory(before, after) >= frame.byteLength);
});
test('linear wasm memory is counted exactly once', () => {
  const before = process.memoryUsage();
  const memory = new WebAssembly.Memory({ initial: 16 });
  const bytes = memory.buffer.byteLength;
  const after = process.memoryUsage();
  assert.equal(after.arrayBuffers - before.arrayBuffers, 0);
  assert.equal(retainedMemory(before, after, bytes), after.heapUsed - before.heapUsed + bytes);
});

test('three equal samples do not hide a delayed loader-buffer release', async () => {
  let time = 0;
  const baseline = await settledMemory({
    collect() {},
    pause: async () => {
      time += 10;
    },
    now: () => time,
    sample: () => ({ arrayBuffers: time < 40 ? 100 : 44, heapUsed: 1000 }),
  });
  assert.equal(time, 140);
  assert.deepEqual(baseline, { arrayBuffers: 44, heapUsed: 1000 });
  assert.equal(retainedMemory(baseline, { arrayBuffers: 44, heapUsed: 1000 }), 0);
});

test('backing stores that never settle fail instead of choosing or clamping a sample', async () => {
  let time = 0;
  await assert.rejects(
    settledMemory({
      collect() {},
      pause: async () => {
        time += 10;
      },
      now: () => time,
      sample: () => ({ arrayBuffers: time, heapUsed: 1000 }),
      timeoutMs: 200,
    }),
    (error) => {
      assert.match(error.message, /Backing-buffer memory did not settle/);
      assert.equal(error.sampleCount, 21);
      assert.equal(error.transitionCount, 20);
      assert.equal(error.lastArrayBuffers, 210);
      assert.equal(error.elapsedMs, 210);
      assert.match(error.message, /samples=21, transitions=20, lastArrayBuffers=210, elapsedMs=210/);
      return true;
    },
  );
});
