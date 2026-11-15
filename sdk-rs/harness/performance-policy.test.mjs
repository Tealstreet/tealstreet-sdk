import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PUBLIC_REPLAY_PAIRS, cpuSlower, median, performanceFailures, performanceMetricErrors, performanceOrder, performanceVerdict, remeasuredVerdict, replayRunCount } from './performance-policy.mjs';

test('public performance requires a complete even interleaved budget', () => {
  assert.equal(replayRunCount(undefined, false), PUBLIC_REPLAY_PAIRS);
  for (const value of ['0', '1', '3', '6', '7.5', '9', '11', '12', '13', '14', '-7', 'NaN', 'Infinity']) assert.throws(() => replayRunCount(value, false));
  assert.equal(replayRunCount('16', false), 16);
  assert.equal(replayRunCount(undefined, true), 1);
  assert.deepEqual(Array.from({ length: 7 }, (_, index) => performanceOrder(index).join(',')),
    ['ts,rust', 'rust,ts', 'ts,rust', 'rust,ts', 'ts,rust', 'rust,ts', 'ts,rust']);
});

const pairs = (values) => Array.from({ length: PUBLIC_REPLAY_PAIRS }, () => ({ ...values }));
const TS = { cpuMs: 200, steadyCpuMs: 200, processCpuMs: 200, retainedBytes: 100 };
const at = (ratio, extra = {}) => pairs({ cpuMs: 200 * ratio, steadyCpuMs: 200 * ratio, processCpuMs: 200 * ratio, retainedBytes: 90, ...extra });

test('Sam 2026-11-07: CPU fails only above 1.00x TS; missing the 10%-faster target warns', () => {
  const clean = performanceVerdict(pairs(TS), at(0.89));
  assert.deepEqual(clean, { failures: [], warnings: [] }, '0.89x is a clean pass');
  const missed = performanceVerdict(pairs(TS), at(0.95));
  assert.deepEqual(missed.failures, [], '0.95x never fails');
  assert.equal(missed.warnings.length, 3);
  for (const warning of missed.warnings) assert.match(warning, /missed the 10%-faster target \(ts=200, rust=190, 0\.950x\)/);
  assert.deepEqual(performanceVerdict(pairs(TS), at(1)).failures, []);
  const slower = performanceVerdict(pairs(TS), at(1.01));
  assert.equal(slower.failures.length, 3);
  for (const failure of slower.failures) assert.match(failure, /Rust is slower than TS \(ts=200, rust=202, 1\.010x\)/);
  assert.ok(cpuSlower(slower.failures));
  assert.ok(!cpuSlower(missed.warnings));
  for (const field of ['cpuMs', 'steadyCpuMs', 'processCpuMs']) {
    const one = performanceVerdict(pairs(TS), at(0.89, { [field]: 202 }));
    assert.equal(one.failures.length, 1, `${field} is an independent CPU objective`);
  }
});

test('one in-job re-measure: 1.01x that clears warns, 1.01x twice fails', () => {
  const first = performanceVerdict(pairs(TS), at(1.01));
  const cleared = remeasuredVerdict(first, performanceVerdict(pairs(TS), at(0.89)));
  assert.deepEqual(cleared.failures, [], 'a miss that clears on re-measure never fails');
  assert.equal(cleared.warnings.length, 3);
  for (const warning of cleared.warnings) assert.match(warning, /Rust is slower than TS \(ts=200, rust=202, 1\.010x\); cleared on the one re-measure/);
  const persisted = remeasuredVerdict(first, performanceVerdict(pairs(TS), at(1.01)));
  assert.equal(persisted.failures.length, 3, '1.01x twice fails');
  assert.ok(cpuSlower(persisted.failures));
  assert.ok(!persisted.warnings.some((warning) => warning.includes('cleared')));
  const nearMiss = remeasuredVerdict(first, performanceVerdict(pairs(TS), at(0.95)));
  assert.deepEqual(nearMiss.failures, []);
  assert.equal(nearMiss.warnings.filter((warning) => warning.includes('10%-faster')).length, 3, 'the re-measured set still reports its own target miss');
  assert.equal(remeasuredVerdict(first, undefined), first, 'no re-measure reports the first set');
});

test('retained memory keeps the 10% rule as a failure; an invalid baseline still fails', () => {
  assert.match(performanceFailures(pairs(TS), at(0.89, { retainedBytes: 90.001 })).join('\n'), /retained memory: Rust must be at least 10% below TS/);
  assert.match(performanceFailures(pairs({ ...TS, retainedBytes: -1 }), at(0.89)).join('\n'), /no valid positive TS baseline/);
  assert.equal(median([4, 1, 7, 3, 6, 5, 2]), 4);
  assert.equal(median([4, 1, 8, 7, 3, 6, 5, 2]), 4.5);
});

test('a CPU re-measure preserves first-set retained-memory failures without duplicates', () => {
  const first = performanceVerdict(pairs(TS), at(1.01, { retainedBytes: 130 }));
  const memory = first.failures.filter((failure) => failure.includes('retained memory'));
  assert.equal(memory.length, 1);
  const cleared = remeasuredVerdict(first, performanceVerdict(pairs(TS), at(0.89)));
  assert.deepEqual(cleared.failures, memory, 'passing re-measured memory cannot erase the first failure');
  assert.equal(cleared.warnings.length, 3, 'the cleared CPU misses remain warnings');
  const persisted = remeasuredVerdict(first, performanceVerdict(pairs(TS), at(0.89, { retainedBytes: 130 })));
  assert.deepEqual(persisted.failures, memory, 'the same failure in both sets is reported once');
  const later = performanceVerdict(pairs(TS), at(1.01, { retainedBytes: 140 }));
  assert.deepEqual(remeasuredVerdict(first, later).failures, [...memory, ...later.failures].sort(),
    'new re-measured CPU and memory failures still bind');
});

test('sub-100 ms CPU objectives fail only above 1.00x and draw no 10%-target warning', () => {
  const ts = pairs({ cpuMs: 4, steadyCpuMs: 60, processCpuMs: 400, retainedBytes: 100 });
  const rust = (values) => pairs({ cpuMs: 4, steadyCpuMs: 60, processCpuMs: 300, retainedBytes: 90, ...values });
  assert.deepEqual(performanceVerdict(ts, rust()), { failures: [], warnings: [] });
  assert.match(performanceFailures(ts, rust({ cpuMs: 4.001 })).join('\n'), /main-thread CPU: Rust is slower than TS/);
  assert.match(performanceFailures(ts, rust({ steadyCpuMs: 60.5 })).join('\n'), /steady-state CPU: Rust is slower than TS/);
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
