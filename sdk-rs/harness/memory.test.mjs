import { test } from 'node:test';
import assert from 'node:assert/strict';
import { memoryMetricErrors, retainedMemory } from './memory.mjs';

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

test('released setup buffers remain a signed component of positive net retention', () => {
  const metrics = metricsFor(
    { heapUsed: 30_271_936, arrayBuffers: 2_324_049 },
    { heapUsed: 33_195_616, arrayBuffers: 2_323_961 },
  );
  assert.equal(metrics.bufferRetainedBytes, -88);
  assert.equal(metrics.retainedBytes, 2_923_592);
  assert.deepEqual(memoryMetricErrors(metrics), []);
});

test('heap shrinkage can accompany larger retained backing buffers and WASM', () => {
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
