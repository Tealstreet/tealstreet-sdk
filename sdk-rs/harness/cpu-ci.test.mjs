import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync, spawnSync } from 'node:child_process';
import { publicCases, validatePlannedCases, requireSecondState, secondMatrix, requireMeasuredProducer, requireAuthorityRefs, classifyInitialCase, requireCertifiedSuite, authenticatedBundle, restorePairedRawOutputs, currentPhysical } from './cpu-ci.mjs';
import { mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { GATE_SUITES, expectedGateCases } from './ci-gate-suites.mjs';

const dispositions = (decision) => [{ key: 'a'.repeat(64), decision,
  case: { ...expectedGateCases('bitget')[0], tapeSha256: 'b'.repeat(64), private: false, runs: 9, correctnessRuns: 1 } }];
const plan = () => ({ schema: 1, runId: 1, checkoutSha: 'c'.repeat(40), suites: GATE_SUITES.map((suite, index) =>
  ({ suite, firstArtifactId: index + 2, firstArtifactDigest: 'sha256:' + 'd'.repeat(64),
    cases: publicCases(suite).map((item, ordinal) => ({
      key: (index * 10 + ordinal).toString(16).padStart(64, '0'), decision: 'first',
      case: { ...item, tapeSha256: 'b'.repeat(64), private: false, runs: 9, correctnessRuns: 1 },
    })),
  })) });
const job = (extra = {}) => ({ id: 3, run_attempt: 1, status: 'completed', conclusion: 'success', ...extra });

test('paired raw restoration preserves bytes at original paths and refuses escapes before writing', t => {
  const root = realpathSync(mkdtempSync(resolve(tmpdir(), 'paired-raw-restoration-')));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const target = resolve(root, 'sdk-rs/harness/out/bitget-10/ts.json');
  const evidence = { report: { artifacts: [{ path: target }] }, raw: [{ bytes: '{"fixture":1}\n' }] };
  restorePairedRawOutputs(root, evidence);
  assert.equal(readFileSync(target, 'utf8'), evidence.raw[0].bytes);
  for (const path of ['/tmp/forbidden-paired-output', '../outside']) {
    const invalid = structuredClone(evidence); invalid.report.artifacts[0].path = path;
    assert.throws(() => restorePairedRawOutputs(root, invalid), /escapes/);
    assert.equal(readFileSync(target, 'utf8'), evidence.raw[0].bytes);
  }
});

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

test('status names incomplete shared certification before reading absent or malformed output', () => {
  for (const result of ['failure', 'cancelled', 'skipped', '']) for (const output of ['', '{not JSON']) {
    const child = spawnSync(process.execPath, ['sdk-rs/harness/cpu-ci.mjs', '--command', 'status', '--suite', 'bitget'], {
      encoding: 'utf8', env: { ...process.env, CPU_CERT_RESULT: result, CPU_CERT_RESULTS: output },
    });
    assert.equal(child.status, 1);
    assert.equal(child.stderr.trim(), `shared CPU certification did not complete (result=${result || 'missing'}); non-blocking`);
    assert.doesNotMatch(child.stdout, /accepted/);
  }
});

test('successful upstream status still requires complete same-run same-checkout certification', () => {
  const checkoutSha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const results = { schema: 1, runId: 123, checkoutSha, suites: Object.fromEntries(GATE_SUITES.map(suite => [suite, true])) };
  const status = value => spawnSync(process.execPath, ['sdk-rs/harness/cpu-ci.mjs', '--command', 'status', '--suite', 'bitget'], {
    encoding: 'utf8', env: { ...process.env, CPU_CERT_RESULT: 'success', CPU_CERT_RESULTS: value, GITHUB_RUN_ID: '123' },
  });
  const pass = status(JSON.stringify(results));
  assert.equal(pass.status, 0, pass.stderr);
  assert.match(pass.stdout, /CPU certification accepted for bitget/);
  for (const mutate of [value => { value.runId++; }, value => { value.checkoutSha = 'f'.repeat(40); },
    value => { delete value.suites.woo; }, value => { value.suites.woo = false; }]) {
    const value = structuredClone(results); mutate(value); const failure = status(JSON.stringify(value));
    assert.equal(failure.status, 1); assert.doesNotMatch(failure.stdout, /accepted/);
  }
  assert.equal(status('').status, 1);
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

test('run 40518541502: the physical producer is re-listed until GitHub records its runner', async () => {
  const name = 'Rust / replay measurement (bitget)', run = { id: 7, run_attempt: 1 };
  const row = runner_id => ({ id: 30, name, run_attempt: 1, runner_id, status: 'in_progress' });
  const physical = (...listings) => {
    const sleeps = [], reader = { getRun: async () => run, listJobs: async () => listings.length > 1 ? listings.shift() : listings[0] };
    return { sleeps, context: { runId: 7, reader, sleep: async ms => { sleeps.push(ms); } } };
  };
  for (const listings of [[[row(0)], [row(null)], [row(40)]], [[], [row(40)]]]) {
    const x = physical(...listings), { job } = await currentPhysical(x.context, name);
    assert.equal(job.runner_id, 40); assert.deepEqual(x.sleeps, Array(listings.length - 1).fill(10_000));
  }
  const unassigned = physical([row(0)]);
  await assert.rejects(currentPhysical(unassigned.context, name),
    /missing unique original physical job; candidates \[\{"id":30,"name":"Rust \/ replay measurement \(bitget\)","run_attempt":1,"runner_id":0,"status":"in_progress"\}\]/);
  assert.equal(unassigned.sleeps.length, 6);
  const absent = physical([]);
  await assert.rejects(currentPhysical(absent.context, name), /candidates \[\]/);
  assert.equal(absent.sleeps.length, 6);
  for (const listing of [[row(40), row(41)], [{ ...row(40), run_attempt: 2 }]]) {
    const x = physical(listing);
    await assert.rejects(currentPhysical(x.context, name), /missing unique original physical job/);
    assert.equal(x.sleeps.length, 0);
  }
});

test('the CPU public roster includes generated public tapes and no private case', () => {
  const binance = publicCases('binance').map((item) => `${item.venue} ${item.tape}`);
  assert.ok(binance.includes('binanceoptions sdk-rs/harness/tapes/binanceoptions.jsonl'));
  assert.ok(binance.includes('binance sdk-rs/harness/fixtures/binance-public.jsonl.gz'));
  assert.ok(binance.every((item) => !item.endsWith('-private.jsonl')));
  assert.equal(binance.length, 5);
});
