import { test } from 'node:test';
import assert from 'node:assert/strict';
import { idleLoadedBaseline, measureBeforeCapture, memoryMetricErrors, retainedMemory, settledMemory } from './memory.mjs';

const metricsFor = (before, after, wasmBytes = 0, wasmBeforeBytes = 0) => ({
  baselineKind: 'idle-loaded', measuredMode: 'performance',
  cpuMs: 5, processCpuMs: 7, steadyCpuMs: 2, wallMs: 20,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 2, idleLoadWallMs: 5,
  runtimeCpuMs: 4, runtimeProcessCpuMs: 5, runtimeWallMs: 15,
  heapBeforeBytes: before.heapUsed, heapUsedBytes: after.heapUsed,
  buffersBefore: before.arrayBuffers, buffersAfter: after.arrayBuffers,
  heapRetainedBytes: after.heapUsed - before.heapUsed,
  bufferRetainedBytes: after.arrayBuffers - before.arrayBuffers,
  wasmBeforeBytes, wasmBytes, wasmRetainedBytes: wasmBytes - wasmBeforeBytes,
  retainedBytes: retainedMemory(before, after, wasmBytes, wasmBeforeBytes),
});

test('retained frame backing buffers remain included when GC releases heap', () => {
  const frameBytes = 16 * 1024 * 1024;
  const before = { heapUsed: 100_000, arrayBuffers: 400_000 };
  const after = { heapUsed: 90_000, arrayBuffers: 400_000 + frameBytes };
  assert.equal(retainedMemory(before, after), frameBytes - 10_000);
  assert.deepEqual(memoryMetricErrors(metricsFor(before, after)), []);
});

test('static WASM reservation is excluded and runtime linear-memory growth counted exactly once', () => {
  const before = { heapUsed: 1000, arrayBuffers: 100 };
  const after = { heapUsed: 1200, arrayBuffers: 200 };
  const page = 65_536;
  const memory = new WebAssembly.Memory({ initial: 16 });
  const wasmBeforeBytes = memory.buffer.byteLength;
  memory.grow(2);
  const wasmBytes = memory.buffer.byteLength;
  assert.equal(retainedMemory(before, after, wasmBytes, wasmBeforeBytes), 300 + 2 * page);
  const metrics = metricsFor(before, after, wasmBytes, wasmBeforeBytes);
  assert.equal(metrics.wasmRetainedBytes, 2 * page);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('both implementations establish their idle baseline after setup, imports and tape loading', async () => {
  for (const impl of ['ts', 'rust']) {
    let memory = { heapUsed: 1000, arrayBuffers: 200 };
    const calls = [];
    const loaded = { impl, module: {} };
    const { memory: before, loaded: result } = await idleLoadedBaseline({
      async load() {
        calls.push('initialize', 'import', 'tape');
        memory = { heapUsed: 9000, arrayBuffers: 2200 };
        await Promise.resolve();
        return loaded;
      },
      async baseline() { calls.push('baseline'); return { ...memory }; },
    });
    calls.push('runtime');
    assert.deepEqual(calls, ['initialize', 'import', 'tape', 'baseline', 'runtime']);
    assert.equal(result, loaded);
    memory = { heapUsed: 9300, arrayBuffers: 2300 };
    assert.equal(metricsFor(before, memory).retainedBytes, 400);
  }
});

test('idle loading and baseline failures propagate before runtime allocation', async () => {
  for (const failed of ['load', 'baseline']) {
    const calls = [];
    await assert.rejects(idleLoadedBaseline({
      async load() { calls.push('load'); if (failed === 'load') throw new Error(failed); return {}; },
      async baseline() { calls.push('baseline'); throw new Error(failed); },
    }), new RegExp(failed));
    assert.deepEqual(calls, failed === 'load' ? ['load'] : ['load', 'baseline']);
  }
});

test('memory sampling waits for implementation loading to finish', async () => {
  let finishLoad;
  let sampled = false;
  const pending = idleLoadedBaseline({
    load: () => new Promise((resolve) => { finishLoad = resolve; }),
    baseline: async () => { sampled = true; return { heapUsed: 1000, arrayBuffers: 100 }; },
  });
  assert.equal(sampled, false);
  const loaded = {};
  finishLoad(loaded);
  assert.equal((await pending).loaded, loaded);
  assert.equal(sampled, true);
});

test('cold loading CPU is measured separately from baseline collection without estimates', async () => {
  let thread = 1000;
  let processCpu = 2000;
  let wall = 10;
  const { loadMetrics } = await idleLoadedBaseline({
    sample: () => ({ thread: { user: thread, system: 0 }, process: { user: processCpu, system: 0 }, wall }),
    async load() { thread += 5000; processCpu += 9000; wall += 20; return {}; },
    async baseline() {
      // GC and idle wait after load must not enter the captured CPU interval.
      thread += 1000000; processCpu += 2000000; wall += 1000;
      return { heapUsed: 1000, arrayBuffers: 100 };
    },
  });
  assert.deepEqual(loadMetrics, { idleLoadCpuMs: 5, idleLoadProcessCpuMs: 9, idleLoadWallMs: 20 });
});

test('negative heap, buffer and aggregate deltas remain signed and valid', () => {
  const variants = [
    metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 900, arrayBuffers: 500 }, 100),
    metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 1500, arrayBuffers: 100 }),
    metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 900, arrayBuffers: 100 }),
  ];
  assert.equal(variants[0].heapRetainedBytes, -100);
  assert.equal(variants[1].bufferRetainedBytes, -100);
  assert.equal(variants[2].retainedBytes, -200);
  for (const metrics of variants) assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('invalid absolute counters and nonfinite deltas fail validation', () => {
  const valid = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 1500, arrayBuffers: 300 }, 100);
  for (const field of ['cpuMs', 'processCpuMs', 'steadyCpuMs', 'wallMs',
    'idleLoadCpuMs', 'idleLoadProcessCpuMs', 'idleLoadWallMs', 'runtimeCpuMs', 'runtimeProcessCpuMs', 'runtimeWallMs', 'heapBeforeBytes', 'heapUsedBytes', 'buffersBefore', 'buffersAfter', 'wasmBeforeBytes', 'wasmBytes', 'wasmRetainedBytes']) {
    for (const value of [-1, NaN, Infinity, -Infinity, null, undefined])
      assert(memoryMetricErrors({ ...valid, [field]: value }).some((error) => error.startsWith(`invalid ${field} (`)), `${field}=${value}`);
  }
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes', 'retainedBytes']) {
    for (const value of [NaN, Infinity, -Infinity, null, undefined])
      assert(memoryMetricErrors({ ...valid, [field]: value }).some((error) => error.startsWith(`invalid ${field} (`)), `${field}=${value}`);
  }
  assert(memoryMetricErrors(undefined).length > 0);
});

test('the required metric schema rejects old before-import baselines and unknown modes', () => {
  const valid = metricsFor({ heapUsed: 0, arrayBuffers: 0 }, { heapUsed: 0, arrayBuffers: 0 });
  assert.deepEqual(memoryMetricErrors(valid), []);
  for (const baselineKind of [undefined, 'before-import', null])
    assert(memoryMetricErrors({ ...valid, baselineKind }).some((error) => error.includes('baselineKind')));
  for (const measuredMode of [undefined, 'audit-free', null])
    assert(memoryMetricErrors({ ...valid, measuredMode }).some((error) => error.includes('measuredMode')));
  assert.deepEqual(memoryMetricErrors({ ...valid, measuredMode: 'correctness' }), []);
});

test('every reported delta and aggregate must exactly match the absolute samples', () => {
  const valid = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 1500, arrayBuffers: 300 }, 300, 200);
  for (const field of ['cpuMs', 'processCpuMs', 'wallMs', 'heapRetainedBytes', 'bufferRetainedBytes', 'wasmRetainedBytes', 'retainedBytes'])
    assert(memoryMetricErrors({ ...valid, [field]: valid[field] + 1 }).some((error) => error.startsWith(`inconsistent ${field} (`)), field);
  assert(memoryMetricErrors({ ...valid, wasmBeforeBytes: 400 }).some((error) => error.startsWith('inconsistent wasmRetainedBytes')));
});

test('performance measures CPU and retained memory before diagnostic reconstruction allocates', async () => {
  const events = [];
  let allocated = 100;
  let cpu = 5;
  const result = await measureBeforeCapture(() => {
    events.push('capture');
    allocated += 10000;
    cpu += 100;
    return { rows: [{ value: 'diagnostic' }] };
  }, async () => {
    events.push('stop-cpu');
    const measuredCpu = cpu;
    await Promise.resolve();
    events.push('sample-memory');
    return { cpuMs: measuredCpu, retainedBytes: allocated };
  });
  assert.deepEqual(events, ['stop-cpu', 'sample-memory', 'capture']);
  assert.deepEqual(result.metrics, { cpuMs: 5, retainedBytes: 100 });
  assert.equal(cpu, 105);
  assert.equal(allocated, 10100);
});

test('performance measurement and reconstruction failures remain fatal', async () => {
  let captured = false;
  await assert.rejects(measureBeforeCapture(() => { captured = true; return {}; }, async () => { throw new Error('measure failed'); }), /measure failed/);
  assert.equal(captured, false);
  await assert.rejects(measureBeforeCapture(() => { throw new Error('capture failed'); }, async () => ({})), /capture failed/);
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
      assert.match(error.message, /Heap\/backing-buffer memory did not settle/);
      assert.equal(error.sampleCount, 21);
      assert.equal(error.transitionCount, 20);
      assert.equal(error.lastArrayBuffers, 210);
      assert.equal(error.elapsedMs, 210);
      assert.match(error.message, /samples=21, transitions=20, lastArrayBuffers=210, elapsedMs=210/);
      return true;
    },
  );
});

test('a delayed heap release restarts quiet time and returns the current sample rather than the minimum', async () => {
  let time = 0;
  const baseline = await settledMemory({
    collect() {},
    pause: async () => { time += 10; },
    now: () => time,
    sample: () => ({ arrayBuffers: 100, heapUsed: time < 80 ? 1000 : time === 80 ? 500 : 520 }),
  });
  assert.equal(time, 180);
  assert.deepEqual(baseline, { arrayBuffers: 100, heapUsed: 520 });
});

test('a heap that keeps reaching new lows fails within the original deadline', async () => {
  let time = 0;
  await assert.rejects(settledMemory({
    collect() {},
    pause: async () => { time += 10; },
    now: () => time,
    sample: () => ({ arrayBuffers: 100, heapUsed: 1000 - time }),
    timeoutMs: 200,
  }), (error) => {
    assert.equal(error.sampleCount, 21);
    assert.equal(error.transitionCount, 0);
    assert.equal(error.heapTransitionCount, 19);
    assert.equal(error.lastHeapUsed, 790);
    assert.equal(error.elapsedMs, 210);
    assert.match(error.message, /heapTransitions=19, lastHeapUsed=790/);
    return true;
  });
});

test('memory sampling awaits collection and propagates collection failure', async () => {
  let finishCollection;
  let paused = false;
  let sampled = false;
  const failure = new Error('collection failed');
  const pending = settledMemory({
    collect: () => new Promise((resolve, reject) => { finishCollection = () => reject(failure); }),
    pause: async () => { paused = true; },
    sample: () => { sampled = true; return { heapUsed: 1000, arrayBuffers: 100 }; },
  });
  assert.equal(paused, false);
  assert.equal(sampled, false);
  finishCollection();
  await assert.rejects(pending, (error) => error === failure);
  assert.equal(paused, false);
  assert.equal(sampled, false);
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
  await assert.rejects(settledMemory({ ...sampling, timeoutMs: 1000 }), /memory did not settle/);
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
  }), /memory did not settle/);
  assert.equal(time, 5100);
});

// Collection diagnostics include the entire awaited collector lifecycle.
test('timeout reports the longest asynchronous collection without changing its deadline', async () => {
  let time = 0;
  await assert.rejects(settledMemory({
    collect: async () => { await Promise.resolve(); time += 25; },
    pause: async () => { time += 5; },
    now: () => time,
    sample: () => ({ arrayBuffers: time, heapUsed: 1000 }),
    timeoutMs: 200,
  }), (error) => {
    assert.equal(error.longestCollectionMs, 25);
    assert.equal(error.elapsedMs, 210);
    assert.equal(error.sampleCount, 7);
    assert.match(error.message, /longestGC=25\.0ms/);
    return true;
  });
});
