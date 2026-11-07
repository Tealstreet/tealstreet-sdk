import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { splitCpuErrors, validatePublicEvidence } from './cpu-remeasure.mjs';
import { PUBLIC_REPLAY_PAIRS, performanceFailures, performanceOrder } from './performance-policy.mjs';

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
const evidence = (q = 1.2, second = false) => {
  const raw = [];
  const add = (impl, mode, run) => {
    const output = { impl, frames: 10, commands: [], transportReceipt: { frames: 10 }, protocolErrors: [],
      markets: [{ symbol: 'BTC' }], tickers: [{ symbol: 'BTC', last: 1 }], books: { BTC: { bids: [[1, 1]] } },
      metrics: metrics(mode, impl === 'ts' ? 200 : 180 * q, impl === 'ts' ? 100 : 80) };
    const bytes = JSON.stringify(output);
    raw.push({ impl, mode, run, bytes, sha256: hash(bytes) });
  };
  for (const impl of ['ts', 'rust']) add(impl, 'correctness', 0);
  for (let run = 0; run < PUBLIC_REPLAY_PAIRS; run++) for (const impl of performanceOrder(run)) add(impl, 'performance', run);
  const samples = Object.fromEntries(['ts', 'rust'].map((impl) => [impl,
    raw.filter((slot) => slot.impl === impl && slot.mode === 'performance').map((slot) => JSON.parse(slot.bytes).metrics)]));
  const failures = performanceFailures(samples.ts, samples.rust).sort();
  const report = { schema: 1, venue: 'example', tape: 'public.jsonl', books: 'BTC', tapeSha256: 'a'.repeat(64), private: false,
    runs: PUBLIC_REPLAY_PAIRS, correctnessRuns: 1, requiredImprovement: 0.1, status: failures.length ? 'FAIL' : 'PASS', failures,
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

test('fixed balanced interleaved pairs and the audited pair are mandatory', () => {
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

const unexpected = (value) => value.report.failures.map((failure) => `example: unexpected ${failure}`);

test('a valid CPU-only miss defers exactly its computed CPU failures', () => {
  const value = evidence();
  assert.equal(value.report.failures.length, 3, 'Rust 216 ms vs TS 200 ms misses all three CPU objectives');
  const errors = [...unexpected(value), 'example: stale accepted difference checkpoint 0 markets'];
  assert.deepEqual(splitCpuErrors(value, errors), { kept: ['example: stale accepted difference checkpoint 0 markets'], deferred: unexpected(value) });
  assert.deepEqual(splitCpuErrors(evidence(0.95), []), { kept: [], deferred: [] }, 'no CPU miss, nothing to defer');
});

test('non-CPU failures, omitted failures and invalid raw metrics defer nothing', () => {
  for (const failure of ['performance retained memory: changed', 'ticker BTC.price: changed', 'invalid baseline']) {
    const value = evidence(); value.report.failures.push(failure);
    assert.deepEqual(splitCpuErrors(value, unexpected(value)).deferred, [], failure);
  }
  const omitted = evidence(); omitted.report.failures.pop();
  assert.deepEqual(splitCpuErrors(omitted, unexpected(omitted)).deferred, []);
  for (const bad of [0, -1, null, 'NaN']) {
    const value = evidence(); alterRaw(value, 2, (output) => { output.metrics.cpuMs = bad; });
    assert.deepEqual(splitCpuErrors(value, unexpected(value)).deferred, []);
  }
  const value = evidence(); alterRaw(value, 2, (output) => { output.metrics.retainedBytes += 1; });
  assert.deepEqual(splitCpuErrors(value, unexpected(value)).deferred, []);
});

test('protocol faults never qualify as a CPU miss', () => {
  for (const mutate of [
    (output) => { output.protocolErrors.push('invalid protocol'); },
    (output) => { output.unserved = 1; },
    (output) => { output.frames = 0; },
    (output) => { output.books = {}; },
    (output) => { output.markets = []; },
    (output) => { output.invalidSignatures = 1; },
  ]) {
    const value = evidence(); alterRaw(value, 2, mutate);
    assert.deepEqual(splitCpuErrors(value, unexpected(value)).deferred, []);
  }
  const malformed = evidence(); malformed.report.artifacts[0] = null;
  assert.doesNotThrow(() => splitCpuErrors(malformed, unexpected(malformed)));
  assert.deepEqual(splitCpuErrors(malformed, unexpected(malformed)).deferred, []);
});
