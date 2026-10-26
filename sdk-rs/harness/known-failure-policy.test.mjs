import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { assessGateReport, assessCorrectnessReport, diagnosticKey, validateKnownFailures } from './known-failure-policy.mjs';
import { valueDifferences } from './snapshot-compare.mjs';

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

test('Lighter daily bootstrap declarations retain exact tape, field, checkpoint and value bounds', () => {
  const declared = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  const entry = declared.failures.find((row) => row.venue === 'lighter' && row.tape.endsWith('/lighter-public.jsonl.gz'));
  const conditional = entry.diagnostics.find((row) => row.condition === 'lighter-zero-bootstrap');
  assert.equal(conditional.keys.length, 588);
  assert.deepEqual(validateKnownFailures(declared), []);
  const value = { ...report(), venue: 'lighter', tapeSha256: entry.tapeSha256, private: false,
    correctnessOnly: true, coverage: { correctness: 'complete', performance: 'not-run' } };
  value.failures = conditional.keys.map((key) => `${key}: ts=0 rust=1.25`);
  assert.deepEqual(assessCorrectnessReport(value, declared).errors, []);
  const key = conditional.keys[0];
  for (const failure of [
    `${key.replace('checkpoint 0', 'checkpoint 1')}: ts=0 rust=1.25`,
    `${key}: ts=1 rust=1.25`,
    `${key}: ts=0 rust=NaN`,
    `${key}: ts=0 rust=Infinity`,
    `${key}: ts=0 rust=1e999`,
    'checkpoint 0 tickers[0].fundingRate: ts=0 rust=1.25',
    'checkpoint 0 tickers[999].volume: ts=0 rust=1.25',
  ]) {
    assert.match(assessCorrectnessReport({ ...value, failures: [failure] }, declared).errors.join('\n'), /unexpected/);
  }
  for (const change of [{ venue: 'other' }, { tapeSha256: '0'.repeat(64) }]) {
    assert.match(assessCorrectnessReport({ ...value, ...change }, declared).errors.join('\n'), /unexpected/);
  }
  for (const field of ['condition', 'venue', 'keys']) {
    const changed = structuredClone(declared);
    const row = changed.failures.find((item) => item.venue === entry.venue && item.tapeSha256 === entry.tapeSha256);
    const rule = row.diagnostics.find((item) => item.condition);
    if (field === 'venue') row.venue = 'other';
    if (field === 'condition') rule.condition = 'any';
    if (field === 'keys') rule.keys = ['checkpoint 1 tickers[0].volume'];
    assert.ok(validateKnownFailures(changed).length);
  }
});

test('a declared final ticker field cannot absorb a second final-only field divergence', () => {
  const value = report();
  const expected = { symbol: 'BTC', fundingRate: 0.01, quoteVolume: 100 };
  value.failures = valueDifferences(expected, { ...expected, fundingRate: 0.02 }, 'ticker BTC');
  const declared = manifest();
  declared.failures[0].diagnostics = [{ key: diagnosticKey(value.failures[0]), divergence: 'Only the existing funding-rate difference is declared.' }];
  assert.deepEqual(assessGateReport(value, declared).errors, []);
  value.failures = valueDifferences(expected, { ...expected, fundingRate: 0.02, quoteVolume: 0 }, 'ticker BTC');
  assert.match(assessGateReport(value, declared).errors.join('\n'), /unexpected ticker BTC.quoteVolume/);
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

test('grouped declarations enumerate exact paths and preserve all exception boundaries', () => {
  const declared = manifest();
  declared.failures[0].diagnostics = [{
    keys: ['account orders[0].clientOrderId', 'account orders[1].clientOrderId'],
    divergence: 'The same native identity omission affects these exact two rows.',
  }];
  assert.deepEqual(assessGateReport(report(), declared).errors, []);
  const value = report();
  value.failures = ['account orders[2].clientOrderId: ts="caller" rust=null'];
  assert.match(assessGateReport(value, declared).errors.join('\n'), /unexpected/);
  for (const mutate of [
    (value) => { value.failures[0].diagnostics[0].keys = []; },
    (value) => { value.failures[0].diagnostics[0].keys = ['account.*']; },
    (value) => { value.failures[0].diagnostics[0].keys = ['same', 'same']; },
    (value) => { value.failures[0].diagnostics[0].keys = 'account.orders'; },
    (value) => { value.failures[0].diagnostics[0].key = 'ambiguous'; },
    (value) => { value.failures[0].diagnostics.push({ key: 'account orders[0].clientOrderId', divergence: 'duplicate across groups' }); },
  ]) { const changed = structuredClone(declared); mutate(changed); assert.ok(validateKnownFailures(changed).length); }
  declared.failures[0].diagnostics[0].keys.push('checkpoint coverage rust: missing checkpoint 2');
  value.failures = ['checkpoint coverage rust: missing checkpoint 2'];
  assert.match(assessGateReport(value, declared).errors.join('\n'), /unexpected/);
});

test('invalid baselines or incomplete workloads cannot become known exceptions', () => {
  const value = report();
  value.failures.push('rust performance run 7: incomplete replay workload');
  const declared = manifest();
  declared.failures[0].diagnostics.push({ key: diagnosticKey(value.failures[1]), divergence: 'Cannot permit missing work.' });
  assert.ok(assessGateReport(value, declared).errors.length);
  const invalid = report(); invalid.samples.ts[0].heapBeforeBytes = -1;
  assert.ok(assessGateReport(invalid, manifest()).errors.length);
  for (const failure of ['fixer modified Rust candidate output', 'example: fixer changed undeclared field account.balance.total', 'checkpoint coverage rust: missing checkpoint 2']) {
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

test('own-audit performance snapshot and duplicate diagnostics cannot be exempted', () => {
  for (const failure of [
    'rust performance run 7 tickers[0].last: ts=100 rust=0',
    'ts performance run 1 markets[0].symbol: ts="BTC" rust="ETH"',
    'rust performance run 2 books.BTC.asks[0][0]: ts=100 rust=101',
    'ts performance run 3 account.balance.total: ts=10 rust=0',
    'rust performance run 4 rust tickers: duplicate symbol BTC at 0 and 1',
    'rust performance run 5 ts: duplicate symbol BTC',
  ]) {
    const value = report();
    value.private = false; value.runs = 7; value.requiredImprovement = 0.10;
    value.samples = { ts: Array.from({ length: 7 }, () => metric('performance', 20)), rust: Array.from({ length: 7 }, () => metric('performance', 10)) };
    value.failures = [failure];
    const declared = manifest();
    declared.failures[0].diagnostics = [{ key: diagnosticKey(failure), divergence: 'Cannot exempt an implementation failing its own audit.' }];
    assert.match(assessGateReport(value, declared).errors.join('\n'), /unexpected/);
  }
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
