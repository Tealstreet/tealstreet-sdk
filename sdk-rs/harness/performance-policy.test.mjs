import assert from 'node:assert/strict';
import { test } from 'node:test';
import { median, performanceFailures, performanceMetricErrors, performanceOrder, replayRunCount, smallWindowRunCount, SMALL_WINDOW_RUNS } from './performance-policy.mjs';

test('public performance requires seven or more complete interleaved pairs', () => {
  assert.equal(replayRunCount(undefined, false), 7);
  for (const value of ['0', '1', '3', '6', '7.5', '-7', 'NaN', 'Infinity']) assert.throws(() => replayRunCount(value, false));
  assert.equal(replayRunCount('8', false), 8);
  assert.equal(replayRunCount(undefined, true), 1);
  assert.deepEqual(Array.from({ length: 7 }, (_, index) => performanceOrder(index).join(',')),
    ['ts,rust', 'rust,ts', 'ts,rust', 'rust,ts', 'ts,rust', 'rust,ts', 'ts,rust']);
});

test('ten percent margin is inclusive and process CPU is an independent gate', () => {
  const sample = { cpuMs: 100, steadyCpuMs: 100, processCpuMs: 100, retainedBytes: 100 };
  const ts = Array.from({ length: 7 }, () => sample);
  const rust = Array.from({ length: 7 }, () => ({ cpuMs: 90, steadyCpuMs: 90, processCpuMs: 90, retainedBytes: 90 }));
  assert.deepEqual(performanceFailures(ts, rust), []);
  for (const field of Object.keys(sample)) {
    const changed = rust.map((value) => ({ ...value, [field]: 90.001 }));
    assert.equal(performanceFailures(ts, changed).length, 1, field);
  }
  assert.match(performanceFailures(ts.map((value) => ({ ...value, retainedBytes: -1 })), rust).join('\n'), /no valid positive TS baseline/);
  assert.equal(median([4, 1, 7, 3, 6, 5, 2]), 4);
  assert.equal(median([4, 1, 8, 7, 3, 6, 5, 2]), 4.5);
});

test('sub-100 ms main and steady CPU objectives allow max(1 ms, 5%); process CPU and memory stay strict', () => {
  const ts = Array.from({ length: 9 }, () => ({ cpuMs: 4, steadyCpuMs: 60, processCpuMs: 4, retainedBytes: 100 }));
  const rust = (values) => Array.from({ length: 9 }, () => ({ cpuMs: 5, steadyCpuMs: 63, processCpuMs: 3.6, retainedBytes: 90, ...values }));
  assert.deepEqual(performanceFailures(ts, rust()), []);
  // 1 ms floor below 20 ms, 5% above it.
  assert.match(performanceFailures(ts, rust({ cpuMs: 5.001 })).join('\n'), /main-thread CPU: Rust must be within max\(1 ms, 5%\)/);
  assert.match(performanceFailures(ts, rust({ steadyCpuMs: 63.001 })).join('\n'), /steady-state CPU/);
  assert.equal(performanceFailures(ts, rust({ processCpuMs: 4 })).length, 1);
  // 100 ms and above keep the 10% rule.
  const large = ts.map((value) => ({ ...value, cpuMs: 100 }));
  assert.match(performanceFailures(large, rust({ cpuMs: 90.5 })).join('\n'), /at least 10% below TS/);
  assert.deepEqual(performanceFailures(large, rust({ cpuMs: 90 })), []);
});

test('sub-100 ms main or steady TS windows get nine pairs; larger ones keep the requested count', () => {
  const samples = (cpuMs, steadyCpuMs) => Array.from({ length: 7 }, () => ({ cpuMs, steadyCpuMs }));
  assert.equal(smallWindowRunCount(7, samples(165, 18.6)), SMALL_WINDOW_RUNS);
  assert.equal(smallWindowRunCount(7, samples(45, 150)), SMALL_WINDOW_RUNS);
  assert.equal(smallWindowRunCount(7, samples(165, 100)), 7);
  assert.equal(smallWindowRunCount(11, samples(5, 5)), 11);
});

test('every raw CPU sample and measurement contract is validated before aggregation', () => {
  const metrics = { baselineKind: 'idle-loaded', measuredMode: 'performance', cpuMs: 1, steadyCpuMs: 1, processCpuMs: 1 };
  assert.deepEqual(performanceMetricErrors(metrics, 'performance'), []);
  for (const field of ['cpuMs', 'steadyCpuMs', 'processCpuMs']) for (const value of [0, -1, undefined, NaN, Infinity])
    assert.ok(performanceMetricErrors({ ...metrics, [field]: value }, 'performance').length);
  assert.ok(performanceMetricErrors({ ...metrics, baselineKind: 'before-import' }, 'performance').length);
  assert.ok(performanceMetricErrors(metrics, 'correctness').length);
});
