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

test('pending collection completes before sampling with no intervening pause', async () => {
  let release;
  const pending = new Promise((resolve) => { release = resolve; });
  const events = [];
  let time = 0, collections = 0, published = false;
  const result = settledMemory({
    collect() { events.push('collect'); return ++collections === 1 ? pending : undefined; },
    pause: async () => { events.push('pause'); time += 10; },
    sample() { events.push('sample'); return { arrayBuffers: 42, heapUsed: 1000 }; },
    now: () => time, quietMs: 0,
  }).then((memory) => { published = true; return memory; });
  await new Promise(setImmediate);
  assert.deepEqual(events, ['pause', 'collect']);
  assert.equal(published, false);
  release();
  assert.deepEqual(await result, { arrayBuffers: 42, heapUsed: 1000 });
  assert.deepEqual(events, ['pause', 'collect', 'sample', 'pause', 'collect', 'sample', 'pause', 'collect', 'sample']);
});

test('default major collector is awaited across its asynchronous task boundary', async () => {
  const original = globalThis.gc;
  let pending = false, completed = 0;
  let time = 0;
  globalThis.gc = async (options) => {
    assert.deepEqual(options, { type: 'major', execution: 'async', flavor: 'last-resort' });
    pending = true;
    await new Promise(setImmediate);
    completed++;
    pending = false;
  };
  try {
    await settledMemory({
      pause: async () => { assert.equal(pending, false); time += 10; },
      sample: () => { assert.equal(pending, false); assert(completed > 0); return { arrayBuffers: 42, heapUsed: 1000 }; },
      now: () => time, quietMs: 0,
    });
    assert.equal(completed, 3);
  } finally { if (original === undefined) delete globalThis.gc; else globalThis.gc = original; }
});

test('collector rejection propagates unchanged and clears the real deadline timer', async () => {
  const timers = () => process.getActiveResourcesInfo().filter((name) => name === 'Timeout').length;
  const before = timers();
  const failure = new Error('collection failed');
  let sampled = false;
  await assert.rejects(settledMemory({
    collect: async () => { await new Promise(setImmediate); throw failure; },
    sample: () => { sampled = true; return { arrayBuffers: 0 }; },
    timeoutMs: 10_000,
  }), (error) => error === failure);
  assert.equal(sampled, false);
  assert.equal(timers(), before);
});

test('a hung collector shares the total real deadline and cannot sample after late completion', async () => {
  const original = globalThis.setTimeout;
  const timers = () => process.getActiveResourcesInfo().filter((name) => name === 'Timeout').length;
  const before = timers();
  let release, entered, sampled = 0, calls = 0;
  const pending = new Promise((resolve) => { release = resolve; });
  const collecting = new Promise((resolve) => { entered = resolve; });
  globalThis.setTimeout = () => { throw new Error('virtual timer cannot own collection deadline'); };
  try {
    const started = performance.now();
    const result = settledMemory({
      collect: () => { calls++; entered(); return pending; },
      sample: () => { sampled++; return { arrayBuffers: 0 }; },
      // This case isolates a hung collector, not time spent in the sampling pause.
      // The real deadline remains live even with a fixed supplied sampling clock.
      pause: async () => {}, now: () => 0,
      timeoutMs: 30,
    });
    await collecting;
    assert.equal(calls, 1);
    await assert.rejects(result, /Backing-buffer memory did not settle/);
    assert(performance.now() - started < 500, 'hung collection exceeded its bounded deadline');
    assert.equal(timers(), before);
    release();
    await new Promise(setImmediate);
    assert.equal(sampled, 0);
    assert.equal(calls, 1);
  } finally { globalThis.setTimeout = original; }
});

test('collection duration consumes the existing budget instead of restarting it', async () => {
  let time = 0, sampled = 0, pauses = 0;
  await assert.rejects(settledMemory({
    collect: async () => { time += 40; },
    pause: async () => { pauses++; time += 10; },
    sample: () => { sampled++; return { arrayBuffers: 42 }; },
    now: () => time, timeoutMs: 100,
  }), /Backing-buffer memory did not settle/);
  assert.equal(time, 110);
  assert.equal(pauses, 3);
  assert.equal(sampled, 2);
});

test('allocations during settlement are collected before the retained sample', async () => {
  let time = 0, uncollected = 0;
  const memory = await settledMemory({
    pause: async () => { time += 10; uncollected = 4_000_000; },
    collect: async () => { await new Promise(setImmediate); uncollected = 0; },
    sample: () => ({ arrayBuffers: 42, heapUsed: 1000 + uncollected }),
    now: () => time, quietMs: 0,
  });
  assert.deepEqual(memory, { arrayBuffers: 42, heapUsed: 1000 });
});

test('a hung settlement pause shares the deadline and cannot start late collection', async () => {
  let release, collections = 0, sampled = 0;
  const pending = new Promise((resolve) => { release = resolve; });
  await assert.rejects(settledMemory({
    pause: () => pending,
    collect: () => { collections++; },
    sample: () => { sampled++; return { arrayBuffers: 0 }; },
    timeoutMs: 30,
  }), /Backing-buffer memory did not settle/);
  release();
  await new Promise(setImmediate);
  assert.equal(collections, 0);
  assert.equal(sampled, 0);
});
