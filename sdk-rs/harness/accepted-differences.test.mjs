import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { ACCEPTED_DIFFERENCES, acceptedCoverageErrors, assessGateReport, validateAcceptedDifferences } from './known-failure-policy.mjs';
import { verifyGateSuite } from './verify-gates.mjs';
import { expectedGateCases } from './ci-gate-suites.mjs';

const metric = (mode = 'correctness', amount = 10) => ({
  baselineKind: 'idle-loaded', measuredMode: mode,
  cpuMs: amount, processCpuMs: amount, steadyCpuMs: amount,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1,
  runtimeCpuMs: amount - 1, runtimeProcessCpuMs: amount - 1, runtimeWallMs: amount - 1, wallMs: amount,
  heapBeforeBytes: 100, heapUsedBytes: 100 + amount, heapRetainedBytes: amount,
  buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0,
  wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0, retainedBytes: amount,
});
const freshness = 'checkpoint 2 account.positions[0].updatedAt';
const observed = `${freshness}: ts=100 rust=250`;
const report = (failures = [observed]) => ({
  schema: 1, venue: 'example', tape: 'sdk-rs/harness/tapes/example-private.jsonl', tapeSha256: 'a'.repeat(64), private: true,
  status: failures.length ? 'FAIL' : 'PASS', failures, correctnessRuns: 1, runs: 0, samples: { ts: [metric()], rust: [metric()] },
});
const none = () => ({ schema: 1, failures: [] });
const accepted = (failures = [observed], extra = {}) => ({ schema: 1, differences: [{
  venue: 'example', tapeSha256: 'a'.repeat(64), tape: 'sdk-rs/harness/tapes/example-private.jsonl',
  diagnostics: [{ failures, category: 'rust-better', rationale: 'Rust stamps the later freshness epoch from the WS frame.',
    evidence: 'sdk-rs/harness/tapes/example-private.jsonl frame 12', ...extra }],
}] });

test('the committed accepted-difference manifest is valid and names current CI tapes', () => {
  const known = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  assert.deepEqual(validateAcceptedDifferences(ACCEPTED_DIFFERENCES, known), []);
  assert.deepEqual(acceptedCoverageErrors(ACCEPTED_DIFFERENCES), []);
});

test('an accepted entry must name a CI gate case', () => {
  const tape = 'sdk-rs/harness/tapes/bitget-private.jsonl';
  const entry = (extra) => ({ schema: 1, differences: [{ ...accepted().differences[0], venue: 'bitget', tape, ...extra }] });
  assert.deepEqual(acceptedCoverageErrors(entry()), []);
  assert.match(acceptedCoverageErrors(entry({ venue: 'example' })).join('\n'), /no CI gate case/);
  assert.match(acceptedCoverageErrors(entry({ tape: 'sdk-rs/harness/tapes/missing.jsonl' })).join('\n'), /no CI gate case/);
});

test('an accepted path pins its observed values: a changed value is a new divergence', () => {
  assert.match(assessGateReport(report([`${freshness}: ts=100 rust=999`]), none(), accepted()).errors.join('\n'), /accepted difference changed value/);
  assert.match(assessGateReport(report([`${freshness}: ts=101 rust=250`]), none(), accepted()).errors.join('\n'), /accepted difference changed value/);
});

test('an exact accepted key classifies as accepted, not known or new', () => {
  const assessment = assessGateReport(report(), none(), accepted());
  assert.deepEqual(assessment.errors, []);
  assert.deepEqual(assessment.known, []);
  assert.deepEqual(assessment.accepted.map((item) => [item.key, item.category]), [[freshness, 'rust-better']]);
});

test('accepted entries are bound to venue, tape hash and exact path', () => {
  for (const mutate of [
    (value) => { value.venue = 'other'; },
    (value) => { value.tapeSha256 = 'b'.repeat(64); },
    (value) => { value.tape = 'sdk-rs/harness/tapes/other-private.jsonl'; },
    (value) => { value.failures.push('checkpoint 2 account.positions[0].contracts: ts=1 rust=2'); },
  ]) {
    const value = report(); mutate(value);
    assert.match(assessGateReport(value, none(), accepted()).errors.join('\n'), /unexpected|stale/);
  }
});

test('a stale accepted difference fails verification', () => {
  assert.deepEqual(assessGateReport(report([]), none(), accepted()).errors, ['example: stale accepted difference ' + freshness]);
});

test('accepted declarations require a category, rationale, evidence and an output-comparison path', () => {
  for (const invalid of [
    accepted([observed], { category: 'debt' }),
    accepted([observed], { rationale: ' ' }),
    accepted([observed], { evidence: '' }),
    accepted([freshness]),
    accepted(['checkpoint 2 account.positions[*].updatedAt: ts=1 rust=2']),
    accepted([observed, `${freshness}: ts=1 rust=2`]),
    accepted(['performance cpuMs: Rust must be at least 10% below TS (ts=1, rust=2)']),
    accepted(['checkpoint coverage replay: count 1; expected 2']),
    accepted(['HTTP write sequence run 1: ts=[] rust=[1]']),
    accepted([observed], { condition: 'lighter-zero-bootstrap' }),
    accepted([observed], { keys: [freshness] }),
    { schema: 2, differences: [] },
  ]) assert.ok(validateAcceptedDifferences(invalid, none()).length, JSON.stringify(invalid));
});

test('one path cannot be both a known failure and an accepted difference', () => {
  const known = { schema: 1, failures: [{ venue: 'example', tapeSha256: 'a'.repeat(64), diagnostics: [
    { key: freshness, divergence: 'Debt at the same path.' },
  ] }] };
  assert.match(validateAcceptedDifferences(accepted(), known).join('\n'), /also a known failure/);
  assert.ok(assessGateReport(report(), known, accepted()).errors.length);
});

test('protected harness diagnostics are never accepted even at an accepted path', () => {
  const value = report([observed, 'checkpoint coverage replay: count 1; expected 2']);
  assert.match(assessGateReport(value, none(), accepted()).errors.join('\n'), /unexpected checkpoint coverage/);
});

test('verifyGateSuite labels a gate whose only differences are accepted', () => {
  const entries = expectedGateCases('mexc').map((item, index) => {
    const isPrivate = !item.books;
    const samples = Object.fromEntries(['ts', 'rust'].map((impl) => [impl, Array.from({ length: isPrivate ? 1 : 9 }, () => ({
      ...metric(isPrivate ? 'correctness' : 'performance', impl === 'ts' ? 200 : 180),
      ...(impl === 'ts' ? { heapUsedBytes: 400, heapRetainedBytes: 300, retainedBytes: 300 } : {}),
    }))]));
    return { report: { ...item, schema: 1, tapeSha256: String(index + 1).repeat(64), private: isPrivate,
      runs: isPrivate ? 0 : 9, correctnessRuns: 1, requiredImprovement: 0.1, samples, status: 'PASS', failures: [] }, exitCode: 0 };
  });
  const target = entries.find((entry) => entry.report.private).report;
  Object.assign(target, { status: 'FAIL', failures: [observed] });
  entries.find((entry) => entry.report === target).exitCode = 1;
  const declared = accepted();
  Object.assign(declared.differences[0], { venue: target.venue, tapeSha256: target.tapeSha256, tape: target.tape });
  const result = verifyGateSuite({ suite: 'mexc', entries, manifest: none(), accepted: declared });
  assert.deepEqual(result.errors, []);
  assert.equal(result.acceptedGates, 1);
  assert.equal(result.knownFailures, 0);
  assert.ok(result.rows.some((row) => row.label === 'PARITY WITH ACCEPTED DIFFERENCES'));
});

test('verifyGateSuite fails an accepted entry whose tape was re-recorded or not replayed', () => {
  const entries = expectedGateCases('mexc').map((item, index) => {
    const isPrivate = !item.books;
    const samples = Object.fromEntries(['ts', 'rust'].map((impl) => [impl, Array.from({ length: isPrivate ? 1 : 9 }, () => ({
      ...metric(isPrivate ? 'correctness' : 'performance', impl === 'ts' ? 200 : 180),
      ...(impl === 'ts' ? { heapUsedBytes: 400, heapRetainedBytes: 300, retainedBytes: 300 } : {}),
    }))]));
    return { report: { ...item, schema: 1, tapeSha256: String(index + 1).repeat(64), private: isPrivate,
      runs: isPrivate ? 0 : 9, correctnessRuns: 1, requiredImprovement: 0.1, samples, status: 'PASS', failures: [] }, exitCode: 0 };
  });
  const target = entries.find((entry) => entry.report.private).report;
  const declared = accepted();
  Object.assign(declared.differences[0], { venue: target.venue, tapeSha256: 'f'.repeat(64), tape: target.tape });
  const result = verifyGateSuite({ suite: 'mexc', entries, manifest: none(), accepted: declared });
  assert.match(result.errors.join('\n'), /was not replayed at that tape SHA/);
  assert.deepEqual(verifyGateSuite({ suite: 'mexc', entries, manifest: none(), accepted: { schema: 1, differences: [] } }).errors, []);
});
