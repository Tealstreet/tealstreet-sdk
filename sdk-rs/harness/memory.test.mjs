import { test } from 'node:test';
import assert from 'node:assert/strict';
import { memoryMetricErrors, retainedMemory, settledMemory } from './memory.mjs';

test('retained frame backing buffers are included', () => {
  const frame = new ArrayBuffer(16 * 1024 * 1024);
  // GC may free heap while a transferred frame remains retained.
  const before = { heapUsed: 100_000, arrayBuffers: 400_000 };
  const after = { heapUsed: 90_000, arrayBuffers: 400_000 + frame.byteLength };
  assert.equal(retainedMemory(before, after), frame.byteLength - 10_000);
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
    /Backing-buffer memory did not settle/,
  );
});

// Incoming venue and protocol regressions.
{
const metricsFor=(before,after,wasmBytes=0)=>({heapBeforeBytes:before.heapUsed,heapUsedBytes:after.heapUsed,buffersBefore:before.arrayBuffers,buffersAfter:after.arrayBuffers,heapRetainedBytes:after.heapUsed-before.heapUsed,bufferRetainedBytes:after.arrayBuffers-before.arrayBuffers,wasmBytes,retainedBytes:retainedMemory(before,after,wasmBytes)});
test('signed buffer retention remains in the exact positive net total', () => {
  const metrics = metricsFor(
    { heapUsed: 30_271_936, arrayBuffers: 2_324_049 },
    { heapUsed: 33_195_616, arrayBuffers: 2_323_961 },
  );
  assert.equal(metrics.bufferRetainedBytes, -88);
  assert.equal(metrics.retainedBytes, 2_923_592);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('signed heap retention combines exactly with buffers and WASM', () => {
  const metrics = metricsFor(
    { heapUsed: 1000, arrayBuffers: 200 },
    { heapUsed: 900, arrayBuffers: 500 },
    100,
  );
  assert.equal(metrics.heapRetainedBytes, -100);
  assert.equal(metrics.retainedBytes, 300);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('invalid absolute counters and nonfinite signed deltas fail validation', () => {
  const valid = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 1500, arrayBuffers: 300 }, 100);
  for (const field of ['heapBeforeBytes', 'heapUsedBytes', 'buffersBefore', 'buffersAfter', 'wasmBytes', 'retainedBytes']) {
    for (const value of [-1, NaN, Infinity, -Infinity, null, undefined]) {
      assert(memoryMetricErrors({ ...valid, [field]: value }).some((error) => error.startsWith(`invalid ${field} (`)), `${field}=${value}`);
    }
  }
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes']) {
    for (const value of [NaN, Infinity, -Infinity, null, undefined]) {
      assert(memoryMetricErrors({ ...valid, [field]: value }).some((error) => error.startsWith(`invalid ${field} (`)), `${field}=${value}`);
    }
  }
  assert(memoryMetricErrors(undefined).length > 0);
});

test('negative aggregate retention still invalidates the baseline', () => {
  const metrics = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 900, arrayBuffers: 250 });
  assert.equal(metrics.retainedBytes, -50);
  assert(memoryMetricErrors(metrics).some((error) => error.startsWith('invalid retainedBytes (')));
});

test('reported deltas and aggregate must exactly match the absolute samples', () => {
  const valid = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 1500, arrayBuffers: 300 }, 100);
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes', 'retainedBytes']) {
    assert(memoryMetricErrors({ ...valid, [field]: valid[field] + 1 }).some((error) => error.startsWith(`inconsistent ${field} (`)), field);
  }
  assert.deepEqual(memoryMetricErrors(metricsFor({ heapUsed: 0, arrayBuffers: 0 }, { heapUsed: 0, arrayBuffers: 0 })), []);
});

test('settled samples use real time when replay replaces global timers', async () => {
  const original = globalThis.setTimeout;
  globalThis.setTimeout = () => { throw new Error('virtual timer must not pace memory samples'); };
  try {
    assert.deepEqual(await settledMemory({
      collect() {}, sample: () => ({ arrayBuffers: 42, heapUsed: 1000 }), quietMs: 0,
    }), { arrayBuffers: 42, heapUsed: 1000 });
  } finally {
    globalThis.setTimeout = original;
  }
});
}
