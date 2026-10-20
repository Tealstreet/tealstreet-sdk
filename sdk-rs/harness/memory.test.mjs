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

const metricsFor = (before, after, wasmBytes = 0) => ({
  heapBeforeBytes: before.heapUsed,
  heapUsedBytes: after.heapUsed,
  buffersBefore: before.arrayBuffers,
  buffersAfter: after.arrayBuffers,
  heapRetainedBytes: after.heapUsed - before.heapUsed,
  bufferRetainedBytes: after.arrayBuffers - before.arrayBuffers,
  wasmBytes,
  retainedBytes: retainedMemory(before, after, wasmBytes),
});

test('negative buffer retention invalidates the before-import baseline', () => {
  const metrics = metricsFor(
    { heapUsed: 30_271_936, arrayBuffers: 2_324_049 },
    { heapUsed: 33_195_616, arrayBuffers: 2_323_961 },
  );
  assert.equal(metrics.bufferRetainedBytes, -88);
  assert.equal(metrics.retainedBytes, 2_923_592);
  assert(memoryMetricErrors(metrics).some((error) => error.startsWith('invalid bufferRetainedBytes (-88)')));
});

test('negative heap retention invalidates the before-import baseline', () => {
  const metrics = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 900, arrayBuffers: 500 }, 100);
  assert.equal(metrics.heapRetainedBytes, -100);
  assert.equal(metrics.retainedBytes, 300);
  assert(memoryMetricErrors(metrics).some((error) => error.startsWith('invalid heapRetainedBytes (-100)')));
});

test('invalid absolute counters and retained components fail validation', () => {
  const valid = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 1500, arrayBuffers: 300 }, 100);
  for (const field of [
    'heapBeforeBytes',
    'heapUsedBytes',
    'buffersBefore',
    'buffersAfter',
    'wasmBytes',
    'retainedBytes',
  ]) {
    for (const value of [-1, NaN, Infinity, -Infinity, null, undefined]) {
      assert(
        memoryMetricErrors({ ...valid, [field]: value }).some((error) => error.startsWith(`invalid ${field} (`)),
        `${field}=${value}`,
      );
    }
  }
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes']) {
    for (const value of [-1, NaN, Infinity, -Infinity, null, undefined]) {
      assert(
        memoryMetricErrors({ ...valid, [field]: value }).some((error) => error.startsWith(`invalid ${field} (`)),
        `${field}=${value}`,
      );
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
    assert(
      memoryMetricErrors({ ...valid, [field]: valid[field] + 1 }).some((error) =>
        error.startsWith(`inconsistent ${field} (`),
      ),
      field,
    );
  }
  assert.deepEqual(
    memoryMetricErrors(metricsFor({ heapUsed: 0, arrayBuffers: 0 }, { heapUsed: 0, arrayBuffers: 0 })),
    [],
  );
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

test('settled samples use real time when replay replaces global timers', async () => {
  const original = globalThis.setTimeout;
  globalThis.setTimeout = () => {
    throw new Error('virtual timer must not pace memory samples');
  };
  try {
    assert.deepEqual(
      await settledMemory({
        collect() {},
        sample: () => ({ arrayBuffers: 42, heapUsed: 1000 }),
        quietMs: 0,
      }),
      { arrayBuffers: 42, heapUsed: 1000 },
    );
  } finally {
    globalThis.setTimeout = original;
  }
});

// A busy worker may need several full GCs to release cold loader storage.
test('cold backing-store cleanup can exceed one second before the first quiet sample', async () => {
  let time = 0;
  const sampling = {
    collect() {},
    pause: async () => { time += 10; },
    now: () => time,
    sample: () => ({ arrayBuffers: time < 1500 ? 2000 - time : 100, heapUsed: 1000 }),
  };
  await assert.rejects(settledMemory({ ...sampling, timeoutMs: 1000 }), /Backing-buffer memory did not settle/);
  time = 0;
  assert.deepEqual(await settledMemory(sampling), { arrayBuffers: 100, heapUsed: 1000 });
  assert.equal(time, 1600);
});

test('the extended startup deadline still rejects continuously changing backing stores', async () => {
  let time = 0;
  await assert.rejects(settledMemory({
    collect() {},
    pause: async () => { time += 100; },
    now: () => time,
    sample: () => ({ arrayBuffers: time, heapUsed: 1000 }),
  }), /Backing-buffer memory did not settle/);
  assert.equal(time, 5100);
});
