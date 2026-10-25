import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { assessEligibility, decideRemeasurement, validatePublicEvidence } from './cpu-remeasure.mjs';
import { performanceFailures, performanceOrder } from './performance-policy.mjs';

const hash = (value) => createHash('sha256').update(value).digest('hex');
const metrics = (mode, cpu = 180, retained = 80) => ({
  baselineKind: 'idle-loaded', measuredMode: mode,
  cpuMs: cpu, processCpuMs: cpu, steadyCpuMs: cpu,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
  runtimeCpuMs: cpu - 1, runtimeProcessCpuMs: cpu - 1, runtimeWallMs: cpu - 1, wallMs: cpu,
  heapBeforeBytes: 100, heapUsedBytes: 100 + retained, heapRetainedBytes: retained,
  buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0,
  wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0, retainedBytes: retained,
});
const evidence = (q = 1.02, second = false) => {
  const raw = [];
  const add = (impl, mode, run) => {
    const output = { impl, frames: 10, commands: [], transportReceipt: { frames: 10 }, protocolErrors: [],
      markets: [{ symbol: 'BTC' }], tickers: [{ symbol: 'BTC', last: 1 }], books: { BTC: { bids: [[1, 1]] } },
      metrics: metrics(mode, impl === 'ts' ? 200 : 180 * q, impl === 'ts' ? 100 : 80) };
    const bytes = JSON.stringify(output);
    raw.push({ impl, mode, run, bytes, sha256: hash(bytes) });
  };
  for (const impl of ['ts', 'rust']) add(impl, 'correctness', 0);
  for (let run = 0; run < 9; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
  const samples = Object.fromEntries(['ts', 'rust'].map((impl) => [impl,
    raw.filter((slot) => slot.impl === impl && slot.mode === 'performance').map((slot) => JSON.parse(slot.bytes).metrics)]));
  const failures = performanceFailures(samples.ts, samples.rust).sort();
  const report = { schema: 1, venue: 'example', tape: 'public.jsonl', books: 'BTC', tapeSha256: 'a'.repeat(64), private: false,
    runs: 9, correctnessRuns: 1, requiredImprovement: 0.1, status: failures.length ? 'FAIL' : 'PASS', failures,
    samples, fixers: [], artifacts: raw.map(({ impl, mode, run }) => ({ impl, mode, run, path: `out/${impl}-${mode}-${run}` })) };
  return { report, raw, execution: { runId: '10', attempt: 1, jobId: second ? '22' : '21', runnerId: second ? '32' : '31',
    imageOS: 'ubuntu24', imageVersion: '20261020.1', architecture: 'x64', nodeVersion: 'v22.22.3',
    nodeSha256: 'b'.repeat(64), inputsSha256: 'c'.repeat(64), policySha256: 'd'.repeat(64), runtimeSha256: 'e'.repeat(64),
    initialCacheSha256: 'f'.repeat(64), finalCacheSha256: '0'.repeat(64),
    startedAt: second ? '2026-10-23T01:02:00Z' : '2026-10-23T01:00:00Z',
    completedAt: second ? '2026-10-23T01:03:00Z' : '2026-10-23T01:01:00Z' } };
};
const alterRaw = (value, index, mutate) => {
  const object = JSON.parse(value.raw[index].bytes); mutate(object);
  value.raw[index].bytes = JSON.stringify(object); value.raw[index].sha256 = hash(value.raw[index].bytes);
};

test('nine complete interleaved pairs and the audited pair are mandatory', () => {
  assert.deepEqual(validatePublicEvidence(evidence()), []);
  for (const mutate of [
    (value) => { value.report.runs = 7; },
    (value) => { value.raw.pop(); },
    (value) => { value.raw[2] = value.raw[3]; },
    (value) => { value.raw.reverse(); },
    (value) => { value.report.samples.ts.pop(); },
    (value) => { value.raw[0].sha256 = '9'.repeat(64); },
    (value) => { value.report.private = true; },
    (value) => { value.report.requiredImprovement = 0; },
  ]) { const value = evidence(); mutate(value); assert.ok(validatePublicEvidence(value).length); }
});

test('only a complete strictly sub-five-percent CPU-only miss is eligible', () => {
  assert.equal(assessEligibility(evidence()).eligible, true);
  assert.equal(assessEligibility(evidence(1)).eligible, false);
  assert.equal(assessEligibility(evidence(1.049999)).eligible, true);
  assert.equal(assessEligibility(evidence(1.05)).eligible, false);
  assert.equal(assessEligibility(evidence(1.051)).eligible, false);
  for (const failure of ['performance retained memory: changed', 'ticker BTC.price: changed', 'invalid baseline']) {
    const value = evidence(); value.report.failures.push(failure);
    assert.equal(assessEligibility(value).eligible, false);
  }
  const omitted = evidence(); omitted.report.failures.pop();
  assert.equal(assessEligibility(omitted).eligible, false);
});

test('each raw metric is checked and cannot disagree with the reported samples', () => {
  for (const bad of [0, -1, null, 'NaN']) {
    const value = evidence(); alterRaw(value, 2, (output) => { output.metrics.cpuMs = bad; });
    assert.equal(assessEligibility(value).eligible, false);
  }
  const value = evidence(); alterRaw(value, 2, (output) => { output.metrics.retainedBytes += 1; });
  assert.equal(assessEligibility(value).eligible, false);
});

test('two-run arithmetic uses all objectives and leaves both strict reports untouched', () => {
  const first = evidence(1.02), second = evidence(0.98, true), before = JSON.stringify([first, second]);
  const result = decideRemeasurement(first, second);
  assert.deepEqual(result.errors, []);
  assert.equal(result.accepted, true);
  assert.equal(result.receipt.rule, 'all-objective-normalized-two-run-mean-v1');
  assert.equal(result.receipt.objectives.length, 3);
  assert.ok(result.receipt.objectives.every((objective) => objective.mean === 1));
  assert.equal(JSON.stringify([first, second]), before);
  assert.equal(first.report.status, 'FAIL');
  assert.equal(decideRemeasurement(first, evidence(1, true)).accepted, false);
  assert.equal(decideRemeasurement(first, evidence(1.05, true)).accepted, false);
});

test('no per-objective cherry-pick or pooled absolute timing', () => {
  const first = evidence(1.04), second = evidence(0.95, true);
  for (let i = 0; i < second.raw.length; i++) alterRaw(second, i, (output) => {
    if (output.impl === 'rust') output.metrics.steadyCpuMs = 180 * 1.04;
  });
  second.report.samples.rust = second.raw.filter((slot) => slot.impl === 'rust' && slot.mode === 'performance').map((slot) => JSON.parse(slot.bytes).metrics);
  second.report.failures = performanceFailures(second.report.samples.ts, second.report.samples.rust).sort(); second.report.status = 'FAIL';
  const result = decideRemeasurement(first, second);
  assert.equal(result.accepted, false);
  assert.match(result.errors.join('\n'), /steadyCpuMs.*mean/);
});

test('only top-level raw metrics may differ; all nested output and fixer changes block', () => {
  for (const mutate of [
    (value) => alterRaw(value, 0, (output) => { output.books.BTC.bids[0][1] = 2; }),
    (value) => alterRaw(value, 19, (output) => { output.transportReceipt.extra = 1; }),
    (value) => alterRaw(value, 2, (output) => { output.books.metrics = 7; }),
    (value) => { value.report.fixers.push({ run: 0, changed: true }); },
    (value) => { value.report.tapeSha256 = '9'.repeat(64); },
  ]) { const second = evidence(0.97, true); mutate(second); assert.equal(decideRemeasurement(evidence(), second).accepted, false); }
});

test('same runner/job, altered runtime, overlapping execution or third attempt fails closed', () => {
  for (const mutate of [
    (value) => { value.execution.runnerId = '31'; },
    (value) => { value.execution.jobId = '21'; },
    (value) => { value.execution.runnerId = '0'; },
    (value) => { value.execution.attempt = 2; },
    (value) => { value.execution.runId = '11'; },
    (value) => { value.execution.imageVersion += '-new'; },
    (value) => { value.execution.runtimeSha256 = '9'.repeat(64); },
    (value) => { value.execution.finalCacheSha256 = '9'.repeat(64); },
    (value) => { value.execution.startedAt = '2026-10-23T01:00:30Z'; },
    (value) => { delete value.execution.nodeSha256; },
  ]) { const second = evidence(0.97, true); mutate(second); assert.equal(decideRemeasurement(evidence(), second).accepted, false); }
});

test('small-window bucket changes block even when the normalized mean passes', () => {
  const second = evidence(0.95, true);
  for (let i = 0; i < second.raw.length; i++) alterRaw(second, i, (output) => {
    output.metrics.steadyCpuMs = output.impl === 'ts' ? 99 : 90;
  });
  for (const impl of ['ts', 'rust']) second.report.samples[impl] = second.raw.filter((slot) => slot.impl === impl && slot.mode === 'performance').map((slot) => JSON.parse(slot.bytes).metrics);
  second.report.failures = performanceFailures(second.report.samples.ts, second.report.samples.rust).sort();
  second.report.status = second.report.failures.length ? 'FAIL' : 'PASS';
  assert.match(decideRemeasurement(evidence(), second).errors.join('\n'), /category/);
});


test('protocol faults and matched corruption in both jobs never qualify as CPU noise', () => {
  for (const mutate of [
    (output) => { output.protocolErrors.push('invalid protocol'); },
    (output) => { output.unserved = 1; },
    (output) => { output.frames = 0; },
    (output) => { output.books = {}; },
    (output) => { output.markets = []; },
    (output) => { output.invalidSignatures = 1; },
  ]) {
    const first = evidence(), second = evidence(0.97, true);
    alterRaw(first, 2, mutate); alterRaw(second, 2, mutate);
    assert.equal(assessEligibility(first).eligible, false);
    assert.equal(decideRemeasurement(first, second).accepted, false);
  }
  const malformed = evidence(); malformed.report.artifacts[0] = null;
  assert.doesNotThrow(() => assessEligibility(malformed));
  assert.equal(assessEligibility(malformed).eligible, false);
});


test('receipt binds exact original report bytes and refuses a parsed-content mismatch', () => {
  const first = evidence(), second = evidence(0.97, true);
  for (const value of [first, second]) value.reportBytes = JSON.stringify(value.report, null, 2) + '\n';
  const result = decideRemeasurement(first, second);
  assert.equal(result.accepted, true);
  assert.equal(result.receipt.strictReports[0].sha256, hash(first.reportBytes));
  assert.notEqual(result.receipt.strictReports[0].sha256, result.receipt.strictReports[0].contentSha256);
  first.reportBytes = '{}';
  assert.equal(decideRemeasurement(first, second).accepted, false);
});

test('a missing, crashed or incomplete second runner is a failure, never first-run acceptance', () => {
  const first = evidence(1.02);
  for (const second of [undefined, null, {}, { error: 'second runner failed' },
    { ...evidence(.95, true), report: { ...evidence(.95, true).report, replayFailure: { message: 'runner crashed' } } }]) {
    const result = decideRemeasurement(first, second);
    assert.equal(result.accepted, false);
    assert.equal(result.receipt, null);
    assert.ok(result.errors.length > 0);
  }
});
