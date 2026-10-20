import assert from 'node:assert/strict';
import { test } from 'node:test';
import { assessGateReport, diagnosticKey, validateKnownFailures } from './known-failure-policy.mjs';

const metric = (mode = 'correctness', amount = 10) => ({
  baselineKind: 'idle-loaded', measuredMode: mode,
  cpuMs: amount, processCpuMs: amount, steadyCpuMs: amount,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
  runtimeCpuMs: amount - 1, runtimeProcessCpuMs: amount - 1, runtimeWallMs: amount - 1, wallMs: amount,
  heapBeforeBytes: 100, heapUsedBytes: 100 + amount, heapRetainedBytes: amount,
  buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0,
  wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0, retainedBytes: amount,
});
const report = () => ({
  schema: 1, venue: 'example', tape: 'fixture.jsonl', tapeSha256: 'a'.repeat(64), private: true,
  status: 'FAIL', failures: ['account orders[0].clientOrderId: ts="caller" rust=null'],
  correctnessRuns: 1, runs: 0, samples: { ts: [metric()], rust: [metric()] },
});
const manifest = () => ({ schema: 1, failures: [{ venue: 'example', tapeSha256: 'a'.repeat(64), diagnostics: [
  { key: 'account orders[0].clientOrderId', divergence: 'The production wire drops the native client ID.' },
] }] });

test('known failures are exact venue, tape hash and field declarations', () => {
  assert.deepEqual(assessGateReport(report(), manifest()).errors, []);
  for (const mutate of [
    (value) => { value.venue = 'other'; },
    (value) => { value.tapeSha256 = 'b'.repeat(64); },
    (value) => { value.failures.push('account orders[0].price: ts=1 rust=2'); },
  ]) { const value = report(); mutate(value); assert.ok(assessGateReport(value, manifest()).errors.length); }
  const passing = report(); passing.status = 'PASS'; passing.failures = [];
  const assessment = assessGateReport(passing, manifest());
  assert.deepEqual(assessment.errors, []);
  assert.equal(assessment.resolved.length, 1);
});

test('manifest rejects wildcards, unnamed exceptions, duplicates and malformed data', () => {
  for (const value of [null, {}, { schema: 1, failures: [null] }]) assert.ok(validateKnownFailures(value).length);
  for (const mutate of [
    (value) => value.failures.push(structuredClone(value.failures[0])),
    (value) => { value.failures[0].diagnostics[0].key = 'account.*'; },
    (value) => { value.failures[0].diagnostics[0].divergence = ''; },
    (value) => value.failures[0].diagnostics.push(null),
  ]) { const value = manifest(); mutate(value); assert.ok(validateKnownFailures(value).length); }
});

test('invalid baselines or incomplete workloads cannot become known exceptions', () => {
  const value = report();
  value.failures.push('rust performance run 7: incomplete replay workload');
  const declared = manifest();
  declared.failures[0].diagnostics.push({ key: diagnosticKey(value.failures[1]), divergence: 'Cannot permit missing work.' });
  assert.ok(assessGateReport(value, declared).errors.length);
  const invalid = report(); invalid.samples.ts[0].heapBeforeBytes = -1;
  assert.ok(assessGateReport(invalid, manifest()).errors.length);
  for (const failure of ['fixer modified Rust candidate output', 'example: fixer changed undeclared field account.balance.total']) {
    const candidate = report(); candidate.failures = [failure];
    const exceptions = manifest(); exceptions.failures[0].diagnostics = [{ key: diagnosticKey(failure), divergence: 'An integrity failure cannot be exempted.' }];
    assert.ok(assessGateReport(candidate, exceptions).errors.length);
  }
});

test('independent verification detects an omitted process CPU failure', () => {
  const value = report();
  value.private = false; value.runs = 7; value.requiredImprovement = 0.10;
  value.samples = { ts: Array.from({ length: 7 }, () => metric('performance', 10)), rust: Array.from({ length: 7 }, () => metric('performance', 10)) };
  assert.match(assessGateReport(value, manifest()).errors.join('\n'), /omitted performance process CPU/);
});

test('declared command timeout remains an incomplete FAIL with no performance evidence', () => {
  const value = report();
  value.failures = ['rust replay: command getAccount did not complete'];
  value.replayFailure = { kind: 'command-timeout', impl: 'rust', command: 'getAccount' };
  value.coverage = { correctness: 'incomplete', performance: 'not-run' };
  value.samples = { ts: [], rust: [] };
  const declared = manifest();
  declared.failures[0].diagnostics = [{ key: value.failures[0], divergence: 'The native venue has no account-query handler.' }];
  assert.deepEqual(assessGateReport(value, declared).errors, []);
  value.status = 'PASS';
  assert.ok(assessGateReport(value, declared).errors.length);
  value.status = 'FAIL'; value.private = false;
  assert.ok(assessGateReport(value, declared).errors.length);
});
