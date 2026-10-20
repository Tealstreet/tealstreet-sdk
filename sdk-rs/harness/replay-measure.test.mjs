import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { baselineBeforeSetup, memoryMetricErrors, retainedMemory } from './memory.mjs';

// Evaluate the actual measurement function without starting a fixture replay.
const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
const measureSource = source.slice(source.indexOf('const measure = '), source.indexOf('const baseline = '));

test('replay reports complete memory samples with CPU before GC and WASM after GC', async () => {
  const events = [];
  const measure = vm.runInNewContext(`${measureSource}\nmeasure`, {
    process: {
      cpuUsage() { events.push('processCPU'); return { user: 1000, system: 1000 }; },
      threadCpuUsage() { events.push('threadCPU'); return { user: 1000, system: 0 }; },
    },
    performance: { now() { events.push('wall'); return 5; } },
    async baseline() { events.push('GC'); return { rss: 2000, heapUsed: 900, arrayBuffers: 500 }; },
    retainedMemory,
  });
  const metrics = await measure({ process: {}, thread: {} }, 0, { heapUsed: 1000, arrayBuffers: 200 },
    () => { events.push('WASM'); return 100; });
  assert.deepEqual(events, ['processCPU', 'threadCPU', 'wall', 'GC', 'WASM']);
  assert.equal(metrics.heapBeforeBytes, 1000);
  assert.equal(metrics.heapRetainedBytes, -100);
  assert.equal(metrics.retainedBytes, 300);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('optional Rust sampling starts after baseline and stops after CPU cutoff before GC', async () => {
  const events = [];
  const { startCpuProfile } = await import('./cpu-profile.mjs');
  const rust = source.indexOf('async function runRust');
  const start = source.indexOf('  const cpuSampler = ', rust);
  const setup = source.slice(source.indexOf('  const heapBefore = await baselineBeforeSetup(', rust), source.indexOf("  const { EngineHost }", rust));
  const end = source.indexOf('    host.start();', start);
  const startup = (setup + source.slice(start, end)).replace("(await import('./cpu-profile.mjs')).startCpuProfile", 'startCpuProfile');
  for (const enabled of [false, true]) {
    events.length = 0;
    let samples = 0;
    const context = {
      values: { 'cpu-profile': enabled ? '/tmp/test.cpuprofile' : undefined },
      process: {
        cpuUsage(start) { events.push(start ? 'processCutoff' : 'processStart'); return { user: 1, system: 0 }; },
        threadCpuUsage(start) { events.push(start ? 'threadCutoff' : 'threadStart'); return { user: 1, system: 0 }; },
      },
      performance: { now() { events.push('wall'); return 5; } },
      async baseline() { events.push('baseline'); return { rss: 2000, heapUsed: 900, arrayBuffers: 500 }; },
      async startCpuProfile(path) {
        assert.equal(enabled, true);
        return startCpuProfile(path, {
          createSession: () => ({ connect() { samples++; },
            async post(method) { if (method === 'Profiler.start' || method === 'Profiler.stop') events.push(method); return { profile: {} }; },
            disconnect() {},
          }),
          writeProfile: async () => {},
        });
      },
      retainedMemory, baselineBeforeSetup, initializeReplay: async () => events.push('initialize'),
    };
    const begun = await vm.runInNewContext(`(async () => { ${startup}\nreturn { cpuSampler, cpuStart, wallStart }; } finally {} })()`, context);
    const measure = vm.runInNewContext(`${measureSource}\nmeasure`, context);
    await measure(begun.cpuStart, begun.wallStart, { heapUsed: 1000, arrayBuffers: 200 },
      () => { events.push('WASM'); return 100; }, undefined, begun.cpuSampler);
    assert.equal(samples, enabled ? 1 : 0);
    assert.deepEqual(events, ['baseline', 'initialize', ...(enabled ? ['Profiler.start'] : []), 'processStart', 'threadStart', 'wall',
      'processCutoff', 'threadCutoff', 'wall', ...(enabled ? ['Profiler.stop'] : []), 'baseline', 'WASM']);
  }
});

test('a failed Rust measurement window still stops the local sampler', async () => {
  const events = [];
  const error = new Error('replay failed');
  const start = source.indexOf('  const cpuSampler = ', source.indexOf('async function runRust'));
  const end = source.indexOf('  const protocolErrors = tape.audit?.errors() ?? [];', start);
  const window = source.slice(start, end).replace("(await import('./cpu-profile.mjs')).startCpuProfile", 'startCpuProfile');
  await assert.rejects(vm.runInNewContext(`(async () => { ${window} })()`, {
    values: { 'cpu-profile': '/tmp/test.cpuprofile' },
    baseline: async () => ({}),
    startCpuProfile: async () => ({ async stop() { events.push('stop'); } }),
    process: { cpuUsage: () => ({}), threadCpuUsage: () => ({}) },
    performance: { now: () => 0 },
    host: { start() { throw error; } },
  }), (actual) => actual === error);
  assert.deepEqual(events, ['stop']);
});

test('sampling cleanup retains a simultaneous replay error before the write error', async () => {
  const replayError = new Error('replay failed');
  const cleanupError = new Error('profile write failed');
  const start = source.indexOf('  const cpuSampler = ', source.indexOf('async function runRust'));
  const end = source.indexOf('  const protocolErrors = tape.audit?.errors() ?? [];', start);
  const window = source.slice(start, end).replace("(await import('./cpu-profile.mjs')).startCpuProfile", 'startCpuProfile');
  await assert.rejects(vm.runInNewContext(`(async () => { ${window} })()`, {
    values: { 'cpu-profile': '/tmp/test.cpuprofile' },
    baseline: async () => ({}),
    startCpuProfile: async () => ({ async stop() { throw cleanupError; } }),
    process: { cpuUsage: () => ({}), threadCpuUsage: () => ({}) },
    performance: { now: () => 0 },
    host: { start() { throw replayError; } },
  }), (actual) => actual.name === 'AggregateError' && actual.errors.length === 2
    && actual.errors[0] === replayError && actual.errors[1] === cleanupError);
});

test('both runtimes snapshot protocol coverage before disposal closes its live audit generations', () => {
  for (const name of ['runRust', 'runTs']) {
    const start = source.indexOf(`async function ${name}()`);
    const snapshot = source.indexOf('  const protocolErrors = tape.audit?.errors() ?? [];', start);
    const restore = source.indexOf('  clock?.restore();', snapshot);
    assert.ok(snapshot > start && restore > snapshot, name);
    let closed = false;
    const dispose = () => { closed = true; };
    const errors = vm.runInNewContext(`${source.slice(snapshot, restore)}\nprotocolErrors`, {
      tape: { audit: { errors: () => closed ? ['missing live subscriptions'] : [] } },
      host: { dispose }, exchange: { dispose },
    });
    assert.equal(closed, true, `${name} actually disposed`);
    assert.deepEqual(Array.from(errors), [], `${name} captures coverage while sockets are alive`);
  }
});


test('optional observer snapshots freeze at CPU cutoff before sampler shutdown and GC', async () => {
  const events = [];
  const live = { cpuMs: 7 };
  const measure = vm.runInNewContext(`${measureSource}\nmeasure`, {
    process: { cpuUsage() { events.push('processCPU'); return { user: 1, system: 0 }; },
      threadCpuUsage() { events.push('threadCPU'); return { user: 1, system: 0 }; } },
    performance: { now() { events.push('wall'); return 5; } },
    async baseline() { events.push('GC'); live.cpuMs = 999; return { rss: 1, heapUsed: 1, arrayBuffers: 1 }; },
    retainedMemory,
  });
  const result = await measure({ process: {}, thread: {} }, 0, { heapUsed: 1, arrayBuffers: 1 }, 0, undefined,
    { async stop() { events.push('stop'); live.cpuMs = 99; } },
    () => { events.push('snapshot'); return { hostProfile: { ...live } }; });
  assert.equal(result.hostProfile.cpuMs, 7);
  assert.deepEqual(events, ['processCPU', 'threadCPU', 'wall', 'snapshot', 'stop', 'GC']);
  assert.equal(live.cpuMs, 999);
});
