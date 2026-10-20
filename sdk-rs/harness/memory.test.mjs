import { test } from 'node:test';
import assert from 'node:assert/strict';
import { baselineBeforeSetup, memoryMetricErrors, retainedMemory, settledMemory } from './memory.mjs';

test('retained frame backing buffers are included', () => {
  const frameBytes = 16 * 1024 * 1024;
  // GC may free heap while a transferred frame remains retained.
  const before = { heapUsed: 100_000, arrayBuffers: 400_000 };
  const after = { heapUsed: 90_000, arrayBuffers: 400_000 + frameBytes };
  assert.equal(retainedMemory(before, after), frameBytes - 10_000);
});
test('linear wasm memory is counted exactly once', () => {
  const before = process.memoryUsage();
  const memory = new WebAssembly.Memory({ initial: 16 });
  const bytes = memory.buffer.byteLength;
  const after = process.memoryUsage();
  // Allocation may collect unrelated buffers, including Node's loaded source.
  assert(after.arrayBuffers <= before.arrayBuffers, 'WASM must not produce observed JS backing-buffer growth');
  assert.equal(
    retainedMemory(before, after, bytes),
    after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + bytes,
  );
  assert.equal(retainedMemory(before, after, bytes) - retainedMemory(before, after, 0), bytes);
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

test('both replay implementations include setup allocations and their later release after the baseline', async () => {
  for (const impl of ['ts', 'rust']) {
    const initial = { heapUsed: 1000, arrayBuffers: 200 };
    const temporary = { heapUsed: 9000, arrayBuffers: 2200 };
    let memory = initial;
    const calls = [];
    const options = { applicationId: '42' };
    const venue = {
      async initializeReplay(selected, supplied) {
        assert.equal(this, venue);
        assert.equal(selected, impl);
        assert.equal(supplied, options);
        calls.push('initialize');
        memory = temporary;
        await Promise.resolve();
        calls.push('initialized');
      },
    };
    const before = await baselineBeforeSetup({
      baseline: async () => {
        calls.push('baseline');
        return { ...memory };
      },
      initialize: () => venue.initializeReplay(impl, options),
    });
    calls.push('load');
    assert.deepEqual(calls, ['baseline', 'initialize', 'initialized', 'load']);
    assert.deepEqual(before, initial);
    memory = { heapUsed: 2000, arrayBuffers: 400 };
    const retained = metricsFor(before, memory);
    assert.equal(retained.retainedBytes, 1200);
    assert.deepEqual(memoryMetricErrors(retained), []);
    assert(memoryMetricErrors(metricsFor(temporary, memory)).some((error) => error.startsWith('invalid retainedBytes')));
  }
});

test('setup waits for the baseline and completes before implementation loading', async () => {
  let finishBaseline;
  let finishSetup;
  let initialized = false;
  let loaded = false;
  const baselineSample = { heapUsed: 1000, arrayBuffers: 200 };
  const pending = baselineBeforeSetup({
    baseline: () => new Promise((resolve) => { finishBaseline = resolve; }),
    initialize: () => {
      initialized = true;
      return new Promise((resolve) => { finishSetup = resolve; });
    },
  }).then((before) => { loaded = true; return before; });
  assert.equal(initialized, false);
  finishBaseline(baselineSample);
  await Promise.resolve();
  assert.equal(initialized, true);
  assert.equal(loaded, false);
  finishSetup();
  assert.equal(await pending, baselineSample);
  assert.equal(loaded, true);
});

test('baseline and setup failures propagate before implementation loading', async () => {
  for (const failure of ['baseline', 'initialize']) {
    const error = new Error(failure);
    let initialized = 0;
    let loaded = false;
    await assert.rejects(async () => {
      await baselineBeforeSetup({
        baseline: async () => {
          if (failure === 'baseline') throw error;
          return { heapUsed: 1000, arrayBuffers: 200 };
        },
        initialize: async () => { initialized++; throw error; },
      });
      loaded = true;
    }, (received) => received === error);
    assert.equal(initialized, failure === 'baseline' ? 0 : 1);
    assert.equal(loaded, false);
  }
});

test('buffer releases can accompany larger retained heap and WASM', () => {
  const metrics = metricsFor(
    { heapUsed: 30_271_936, arrayBuffers: 2_324_049 },
    { heapUsed: 33_195_616, arrayBuffers: 2_323_961 },
  );
  assert.equal(metrics.bufferRetainedBytes, -88);
  assert.equal(metrics.retainedBytes, 2_923_592);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('heap shrinkage can accompany larger retained backing buffers and WASM', () => {
  const metrics = metricsFor({ heapUsed: 1000, arrayBuffers: 200 }, { heapUsed: 900, arrayBuffers: 500 }, 100);
  assert.equal(metrics.heapRetainedBytes, -100);
  assert.equal(metrics.retainedBytes, 300);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('invalid absolute counters and nonfinite signed deltas fail validation', () => {
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
    for (const value of [NaN, Infinity, -Infinity, null, undefined]) {
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

test('settling permits delayed cleanup beyond one second while retaining 100ms quiescence', async () => {
  let time = 0;
  const memory = await settledMemory({
    collect() {},
    pause: async () => { time += 10; },
    now: () => time,
    sample: () => ({ arrayBuffers: time < 1500 ? time : 44, heapUsed: 1000 }),
  });
  assert.equal(time, 1600);
  assert.equal(memory.arrayBuffers, 44);
});

test('elapsed quiescence also requires two equal comparisons after the first sample', async () => {
  let time = 0;
  await settledMemory({
    collect() {},
    pause: async () => { time += 110; },
    now: () => time,
    sample: () => ({ arrayBuffers: 44, heapUsed: 1000 }),
  });
  assert.equal(time, 330);
});


test('private net releases remain informational while absolute counters and arithmetic stay checked', () => {
  const released = metricsFor({ heapUsed: 3000, arrayBuffers: 1000 }, { heapUsed: 1000, arrayBuffers: 1000 });
  assert.equal(released.retainedBytes, -2000);
  assert(memoryMetricErrors(released).some((error) => error.startsWith('invalid retainedBytes (-2000)')));
  assert.deepEqual(memoryMetricErrors(released, { allowNegativeRetained: true }), []);
  assert(memoryMetricErrors({ ...released, heapUsedBytes: -1 }, { allowNegativeRetained: true })
    .some((error) => error.startsWith('invalid heapUsedBytes (-1)')));
  assert(memoryMetricErrors({ ...released, retainedBytes: -1999 }, { allowNegativeRetained: true })
    .some((error) => error.startsWith('inconsistent retainedBytes (-1999')));
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
  assert.equal(time, 10100);
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
