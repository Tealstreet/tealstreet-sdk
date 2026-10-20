import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { memoryMetricErrors, retainedMemory } from './memory.mjs';

// Evaluate the actual measurement function without starting a fixture replay.
const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
const measureSource = source.slice(source.indexOf('const measure = '), source.indexOf('/** Loaded code'));

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
