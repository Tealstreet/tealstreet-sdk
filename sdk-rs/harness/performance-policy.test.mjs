import assert from 'node:assert/strict';
import { test } from 'node:test';
import { median, performanceFailures, performanceMetricErrors, performanceOrder, replayRunCount } from './performance-policy.mjs';

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

test('sub-10 ms main and steady CPU objectives allow 1 ms; process CPU and memory stay strict', () => {
  const ts = Array.from({ length: 7 }, () => ({ cpuMs: 4, steadyCpuMs: 9.9, processCpuMs: 4, retainedBytes: 100 }));
  const rust = (values) => Array.from({ length: 7 }, () => ({ cpuMs: 5, steadyCpuMs: 10.9, processCpuMs: 3.6, retainedBytes: 90, ...values }));
  assert.deepEqual(performanceFailures(ts, rust()), []);
  assert.match(performanceFailures(ts, rust({ cpuMs: 5.001 })).join('\n'), /main-thread CPU: Rust must be within 1 ms/);
  assert.match(performanceFailures(ts, rust({ steadyCpuMs: 10.901 })).join('\n'), /steady-state CPU/);
  assert.equal(performanceFailures(ts, rust({ processCpuMs: 4 })).length, 1);
  const large = ts.map((value) => ({ ...value, cpuMs: 10 }));
  assert.match(performanceFailures(large, rust({ cpuMs: 9.5 })).join('\n'), /at least 10% below TS/);
});

test('every raw CPU sample and measurement contract is validated before aggregation', () => {
  const metrics = { baselineKind: 'idle-loaded', measuredMode: 'performance', cpuMs: 1, steadyCpuMs: 1, processCpuMs: 1 };
  assert.deepEqual(performanceMetricErrors(metrics, 'performance'), []);
  for (const field of ['cpuMs', 'steadyCpuMs', 'processCpuMs']) for (const value of [0, -1, undefined, NaN, Infinity])
    assert.ok(performanceMetricErrors({ ...metrics, [field]: value }, 'performance').length);
  assert.ok(performanceMetricErrors({ ...metrics, baselineKind: 'before-import' }, 'performance').length);
  assert.ok(performanceMetricErrors(metrics, 'correctness').length);
});
