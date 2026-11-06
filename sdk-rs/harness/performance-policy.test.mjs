import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PUBLIC_REPLAY_PAIRS, median, performanceFailures, performanceMetricErrors, performanceOrder, replayRunCount } from './performance-policy.mjs';

test('public performance requires a complete even interleaved budget', () => {
  assert.equal(replayRunCount(undefined, false), PUBLIC_REPLAY_PAIRS);
  for (const value of ['0', '1', '3', '6', '7.5', '9', '11', '12', '13', '14', '-7', 'NaN', 'Infinity']) assert.throws(() => replayRunCount(value, false));
  assert.equal(replayRunCount('16', false), 16);
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
  const ts = Array.from({ length: PUBLIC_REPLAY_PAIRS }, () => ({ cpuMs: 4, steadyCpuMs: 60, processCpuMs: 4, retainedBytes: 100 }));
  const rust = (values) => Array.from({ length: PUBLIC_REPLAY_PAIRS }, () => ({ cpuMs: 5, steadyCpuMs: 63, processCpuMs: 3.6, retainedBytes: 90, ...values }));
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


test('every raw CPU sample and measurement contract is validated before aggregation', () => {
  const metrics = { baselineKind: 'idle-loaded', measuredMode: 'performance', cpuMs: 1, steadyCpuMs: 1, processCpuMs: 1 };
  assert.deepEqual(performanceMetricErrors(metrics, 'performance'), []);
  for (const field of ['cpuMs', 'steadyCpuMs', 'processCpuMs']) for (const value of [0, -1, undefined, NaN, Infinity])
    assert.ok(performanceMetricErrors({ ...metrics, [field]: value }, 'performance').length);
  assert.ok(performanceMetricErrors({ ...metrics, baselineKind: 'before-import' }, 'performance').length);
  assert.ok(performanceMetricErrors(metrics, 'correctness').length);
});

test('ordinary public measurements have a fixed even budget, independent of timing results', () => {
  assert.equal(replayRunCount(undefined, false), 16);
  for (const count of ['7', '9', '11', '12', '13', '14']) assert.throws(() => replayRunCount(count, false));
  for (const count of [16, 18]) {
    const first = Array.from({ length: count }, (_, i) => performanceOrder(i)[0]);
    assert.equal(first.filter((impl) => impl === 'ts').length, count / 2);
    assert.equal(replayRunCount(String(count), false), count);
  }
});
