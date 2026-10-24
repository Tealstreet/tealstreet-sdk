import assert from 'node:assert/strict';
import { test } from 'node:test';
import { validatePlannedCases, requireSecondState, secondMatrix, requireMeasuredProducer, requireAuthorityRefs, classifyInitialCase, requireCertifiedSuite, authenticatedBundle } from './cpu-ci.mjs';
import { GATE_SUITES, expectedGateCases } from './ci-gate-suites.mjs';

const dispositions = (decision) => [{ key: 'a'.repeat(64), decision,
  case: { ...expectedGateCases('bitget')[0], tapeSha256: 'b'.repeat(64), private: false, runs: 9, correctnessRuns: 1 } }];
const plan = () => ({ schema: 1, runId: 1, checkoutSha: 'c'.repeat(40), suites: GATE_SUITES.map((suite, index) =>
  ({ suite, firstArtifactId: index + 2, firstArtifactDigest: 'sha256:' + 'd'.repeat(64),
    cases: expectedGateCases(suite).filter(item => item.tape.endsWith('-public.jsonl.gz')).map((item, ordinal) => ({
      key: (index * 10 + ordinal).toString(16).padStart(64, '0'), decision: 'first',
      case: { ...item, tapeSha256: 'b'.repeat(64), private: false, runs: 9, correctnessRuns: 1 },
    })),
  })) });
const job = (extra = {}) => ({ id: 3, run_attempt: 1, status: 'completed', conclusion: 'success', ...extra });

test('a first PASS needs no second and cannot silently gain another timing sample', () => {
  assert.equal(requireSecondState(dispositions('first'), []), false);
  assert.equal(requireSecondState(dispositions('first'), [job({ conclusion: 'skipped' })]), false);
  for (const value of [[job()], [job({ status: 'queued', conclusion: null })], [job({ conclusion: 'failure' })]])
    assert.throws(() => requireSecondState(dispositions('first'), value), /without a first miss/);
});

test('eligible first miss requires exactly one successful original second; every incomplete/error/retry fails', () => {
  assert.equal(requireSecondState(dispositions('second'), [job()]), true);
  for (const value of [undefined, [], [job({ conclusion: 'failure' })], [job({ conclusion: 'cancelled' })],
    [job({ status: 'in_progress', conclusion: null })], [job({ run_attempt: 2 })], [job(), job({ id: 4 })]])
    assert.throws(() => requireSecondState(dispositions('second'), value));
});

test('the global plan binds all complete public rosters and rejects missing/duplicate suites or reused pairs', () => {
  assert.equal(validatePlannedCases(plan()).suites.length, 32);
  const changes = [p => { p.suites = []; }, p => { p.suites.push(structuredClone(p.suites[0])); },
    p => { p.suites[0].suite = 'unknown'; }, p => { p.suites[0].cases = []; },
    p => { p.suites[0].cases.push(structuredClone(p.suites[0].cases[0])); },
    p => { p.suites[0].cases[0].case.books = 'different'; }, p => { p.suites[0].cases[0].decision = 'reuse'; },
    p => { p.suites[0].cases[0].case.private = true; }, p => { p.suites[0].cases[0].case.runs = 7; },
    p => { p.checkoutSha = ''; }, p => { p.suites[0].firstArtifactDigest = ''; }];
  for (const change of changes) { const value = plan(); change(value); assert.throws(() => validatePlannedCases(value)); }
});


test('only eligible suites enter the second matrix; complete PASS means zero second runners', () => {
  const value = plan();
  assert.deepEqual(secondMatrix(value), { include: [] });
  for (const suite of ['binance', 'bitgetusdc', 'bybit', 'okx']) value.suites.find(row => row.suite === suite).cases[0].decision = 'second';
  assert.deepEqual(secondMatrix(value), { include: ['binance', 'bitgetusdc', 'bybit', 'okx'].map(suite => ({ suite })) });
  value.suites.find(row => row.suite === 'okx').cases[1].decision = 'second';
  assert.equal(secondMatrix(value).include.filter(row => row.suite === 'okx').length, 1, 'one physical second job for multiple eligible public cases');
  value.suites.pop(); assert.throws(() => secondMatrix(value), /suite roster/);
});

test('current upload producer binds first and second execution; old same-source payload cannot replace a measurement', () => {
  const checkoutSha = 'c'.repeat(40), runId = 1, suite = 'bitget';
  const first = { run_id: 1, id: 2, runner_id: 3, run_attempt: 1, name: 'Rust / replay measurement (bitget)' };
  const evidence = { claim: { runId, suite, measuredCheckoutSha: checkoutSha, firstJobId: 2 },
    execution: { runId: '1', jobId: '2', runnerId: '3', measuredCheckoutSha: checkoutSha, attempt: 1 } };
  const args = { runId, checkoutSha, suite, evidence, producer: first, role: 'first' };
  assert.doesNotThrow(() => requireMeasuredProducer(args));
  for (const change of [a => { a.evidence.execution.runId = '9'; a.evidence.claim.runId = 9; },
    a => { a.evidence.execution.jobId = '9'; a.evidence.claim.firstJobId = 9; },
    a => { a.evidence.execution.runnerId = '9'; }, a => { a.producer.run_attempt = 2; },
    a => { a.evidence.claim.suite = 'bitgetusdc'; }, a => { a.producer.name = 'Rust / CPU remeasurement (bitget)'; }]) {
    const bad = structuredClone(args); change(bad); assert.throws(() => requireMeasuredProducer(bad));
  }
  const second = structuredClone(args); second.role = 'second'; second.producer = { ...first, id: 4, runner_id: 5, name: 'Rust / CPU remeasurement (bitget)' };
  second.evidence.execution.jobId = '4'; second.evidence.execution.runnerId = '5';
  assert.doesNotThrow(() => requireMeasuredProducer(second));
  second.evidence.execution.jobId = '2'; assert.throws(() => requireMeasuredProducer(second));
});

test('second authority requires all four canonical artifact refs, including original runtime', () => {
  const originals = Object.fromEntries(['plan', 'strict', 'tiny', 'runtime'].map((kind, index) => [kind,
    { meta: { id: index + 1, name: kind, digest: 'sha256:' + 'd'.repeat(64), size_in_bytes: 20 } }]));
  const authority = Object.fromEntries(Object.entries(originals).map(([kind, { meta }]) => [kind,
    { artifactId: meta.id, name: meta.name, digest: meta.digest, bytes: meta.size_in_bytes }]));
  assert.doesNotThrow(() => requireAuthorityRefs({ authority }, originals));
  for (const kind of Object.keys(originals)) {
    const missing = structuredClone(authority); delete missing[kind]; assert.throws(() => requireAuthorityRefs({ authority: missing }, originals));
    const wrong = structuredClone(authority); wrong[kind].artifactId++; assert.throws(() => requireAuthorityRefs({ authority: wrong }, originals));
  }
});

test('declared public diagnostics keep their strict FAIL and never request a CPU rescue', () => {
  const metric = (cpu, retained) => ({ baselineKind: 'idle-loaded', measuredMode: 'performance',
    cpuMs: cpu, steadyCpuMs: cpu, processCpuMs: cpu, wallMs: cpu,
    idleLoadCpuMs: 0, idleLoadProcessCpuMs: 0, idleLoadWallMs: 0,
    runtimeCpuMs: cpu, runtimeProcessCpuMs: cpu, runtimeWallMs: cpu,
    heapBeforeBytes: 100, heapUsedBytes: 100 + retained, buffersBefore: 0, buffersAfter: 0,
    wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0,
    heapRetainedBytes: retained, bufferRetainedBytes: 0, retainedBytes: retained });
  const report = { schema: 1, venue: 'fixture', tapeSha256: 'b'.repeat(64), private: false, status: 'FAIL',
    requiredImprovement: .1, failures: ['snapshot fixture mismatch'], correctnessRuns: 1, runs: 9,
    samples: { ts: Array(9).fill(metric(200, 100)), rust: Array(9).fill(metric(100, 50)) } };
  const manifest = { schema: 1, failures: [{ venue: 'fixture', tapeSha256: report.tapeSha256,
    diagnostics: [{ key: 'snapshot fixture mismatch', divergence: 'existing declared fixture divergence' }] }] };
  assert.equal(classifyInitialCase({ report }, manifest).decision, 'first');
  assert.equal(report.status, 'FAIL');
  report.failures.push('performance main-thread CPU: fake');
  assert.throws(() => classifyInitialCase({ report }, manifest));
});


test('required check wrappers accept only successful complete same-run same-checkout certification', () => {
  const args = { suite: 'bitgetusdc', result: 'success', runId: 1, checkoutSha: 'a'.repeat(40),
    results: { schema: 1, runId: 1, checkoutSha: 'a'.repeat(40), suites: Object.fromEntries(GATE_SUITES.map(suite => [suite, true])) } };
  assert.equal(requireCertifiedSuite(args), true);
  for (const change of [value => { value.result = 'failure'; }, value => { value.result = 'skipped'; },
    value => { value.results = null; }, value => { value.results.runId = 2; }, value => { value.results.checkoutSha = 'b'.repeat(40); },
    value => { delete value.results.suites.woo; }, value => { value.results.suites.woo = false; },
    value => { value.results.suites.unknown = true; }, value => { value.suite = 'unknown'; }]) {
    const bad = structuredClone(args); change(bad); assert.throws(() => requireCertifiedSuite(bad));
  }
});


test('only a registered runtime artifact receives the runtime transport bound', async () => {
  const exercise = async (name, expected) => {
    const calls = [], run = { id: 1, head_sha: 'c'.repeat(40), head_branch: 'feat/cpu' };
    const meta = { id: 2, name, expired: false, size_in_bytes: 743_167_768,
      digest: 'sha256:' + 'd'.repeat(64), workflow_run: { ...run } };
    const context = { runId: 1, reader: {
      getRun: async () => run, listJobs: async () => [], listArtifacts: async () => [meta],
      getArtifactBytes: async (id, kind) => { calls.push({ id, kind }); throw new Error('controlled download boundary'); },
    } };
    await assert.rejects(authenticatedBundle(context, 1, name, 'producer', 'upload', run.head_sha),
      expected === null ? /CPU remeasurement is not enabled for suite/ : /controlled download boundary/);
    assert.deepEqual(calls, expected === null ? [] : [{ id: 2, kind: expected }]);
  };
  for (const suite of GATE_SUITES) await exercise(`cpu-runtime-${suite}`, 'runtime-artifact');
  for (const name of ['strict-replay-bybit', 'cpu-cohort-bybit', 'cpu-second-bybit', 'cpu-plan', 'cpu-final'])
    await exercise(name, 'artifact');
  for (const name of ['cpu-runtime-bybit-extra', 'cpu-runtime-', 'cpu-runtime-unknown']) await exercise(name, null);
});
