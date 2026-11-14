import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as policy from './performance-policy.mjs';
import { assessGateReport } from './known-failure-policy.mjs';

const samples = (steadyRatio, extra = {}) => Array.from({ length: policy.PUBLIC_REPLAY_PAIRS }, () => ({
  cpuMs: 160, processCpuMs: 160, steadyCpuMs: 200 * steadyRatio, retainedBytes: 80, ...extra,
}));
const ts = samples(1, { cpuMs: 200, processCpuMs: 200, retainedBytes: 100 });
const verdict = (venue, ratio, extra) => policy.performanceVerdict(ts, samples(ratio, extra), venue);

for (const ratio of [1.00, 1.19, 1.20]) test(`HyroTrader accepts raw steady CPU at ${ratio.toFixed(2)}x TS`, () => {
  assert.deepEqual(verdict('hyrotrader', ratio).failures, []);
});

test('HyroTrader refuses raw steady CPU at 1.21x TS', () => {
  const failures = verdict('hyrotrader', 1.21).failures;
  assert.equal(failures.length, 1);
  assert.match(failures[0], /performance steady-state CPU: Rust is slower than TS .*1\.210x/);
});

test('every other venue retains the exact 1.00x raw steady CPU bound', () => {
  for (const venue of [undefined, 'asterdex', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'bitget', 'HyroTrader', 'toString']) {
    assert.deepEqual(verdict(venue, 1).failures, [], String(venue));
    assert.equal(verdict(venue, 1.00001).failures.length, 1, String(venue));
    assert.equal(verdict(venue, 1.19).failures.length, 1, String(venue));
  }
});

test('the per-venue raw steady CPU override table contains only HyroTrader', () => {
  assert.deepEqual(policy.STEADY_CPU_FAIL_RATIOS, { hyrotrader: 1.20 });
  assert.ok(Object.isFrozen(policy.STEADY_CPU_FAIL_RATIOS));
});

test('HyroTrader main-thread CPU, process CPU, memory and warnings are unchanged', () => {
  for (const field of ['cpuMs', 'processCpuMs']) {
    const failures = verdict('hyrotrader', 1.19, { [field]: 200.001 }).failures;
    assert.equal(failures.length, 1);
    assert.doesNotMatch(failures[0], /steady-state CPU/);
  }
  assert.match(verdict('hyrotrader', 1.19, { retainedBytes: 90.001 }).failures.join('\n'), /retained memory/);
  assert.equal(verdict('hyrotrader', 1.19).warnings.length, 1, 'the 10%-faster target still warns');
});

test('the HyroTrader override applies to raw small windows without changing other venues', () => {
  const reference = samples(0.3, { retainedBytes: 100 });
  const candidate = samples(0.36);
  assert.deepEqual(policy.performanceVerdict(reference, candidate, 'hyrotrader').failures, []);
  assert.equal(policy.performanceVerdict(reference, samples(0.363), 'hyrotrader').failures.length, 1);
  assert.equal(policy.performanceVerdict(reference, candidate, 'asterdex').failures.length, 1);
});

test('the one remeasure uses the same HyroTrader boundary', () => {
  const first = verdict('hyrotrader', 1.21);
  const cleared = policy.remeasuredVerdict(first, verdict('hyrotrader', 1.20));
  assert.deepEqual(cleared.failures, []);
  assert.ok(cleared.warnings.some(value => value.includes('cleared on the one re-measure')));
  assert.equal(policy.remeasuredVerdict(first, verdict('hyrotrader', 1.21)).failures.length, 1);
});

test('strict report validation uses the exact venue threshold too', () => {
  const complete = values => values.map(value => ({
    ...value, baselineKind: 'idle-loaded', measuredMode: 'performance', wallMs: 200,
    idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
    runtimeCpuMs: value.cpuMs - 1, runtimeProcessCpuMs: value.processCpuMs - 1, runtimeWallMs: 199,
    heapBeforeBytes: 100, heapUsedBytes: 100 + value.retainedBytes, heapRetainedBytes: value.retainedBytes,
    buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0,
    wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0,
  }));
  const report = venue => {
    const measurements = { ts: complete(ts), rust: complete(samples(1.20)) };
    const failures = policy.performanceFailures(measurements.ts, measurements.rust, venue);
    return { schema: 1, venue, tapeSha256: 'a'.repeat(64), private: false,
      runs: policy.PUBLIC_REPLAY_PAIRS, correctnessRuns: 1, requiredImprovement: 0.10,
      status: failures.length ? 'FAIL' : 'PASS', failures, samples: measurements };
  };
  const declarations = { schema: 1, failures: [] };
  assert.deepEqual(assessGateReport(report('hyrotrader'), declarations).errors, []);
  assert.match(assessGateReport(report('asterdex'), declarations).errors.join('\n'), /unexpected performance steady-state CPU/);
});
