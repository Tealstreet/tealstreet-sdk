import { PUBLIC_REPLAY_PAIRS } from './performance-policy.mjs';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync, spawnSync } from 'node:child_process';
import { publicCases, validatePlannedCases, requireSecondState, secondMatrix, requireMeasuredProducer, requireAuthorityRefs, classifyInitialCase, requireCertifiedSuite, authenticatedBundle, restorePairedRawOutputs, currentPhysical, imageRollReceipt, imageRolledRefusal, certifySuites } from './cpu-ci.mjs';
import { mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { GATE_SUITES, expectedGateCases } from './ci-gate-suites.mjs';

const dispositions = (decision) => [{ key: 'a'.repeat(64), decision,
  case: { ...expectedGateCases('bitget')[0], tapeSha256: 'b'.repeat(64), private: false, runs: PUBLIC_REPLAY_PAIRS, correctnessRuns: 1 } }];
const plan = () => ({ schema: 1, runId: 1, checkoutSha: 'c'.repeat(40), suites: GATE_SUITES.map((suite, index) =>
  ({ suite, firstArtifactId: index + 2, firstArtifactDigest: 'sha256:' + 'd'.repeat(64),
    cases: publicCases(suite).map((item, ordinal) => ({
      key: (index * 10 + ordinal).toString(16).padStart(64, '0'), decision: 'first',
      case: { ...item, tapeSha256: 'b'.repeat(64), private: false, runs: PUBLIC_REPLAY_PAIRS, correctnessRuns: 1 },
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

test('one suite whose first evidence fails refuses only its own CPU gate', () => {
  const value = plan(), index = value.suites.findIndex(row => row.suite === 'bitget');
  value.suites[index] = { suite: 'bitget', refused: 'Bitget paired CI: missing/ambiguous exact open or merged PR for this run' };
  value.suites.find(row => row.suite === 'okx').cases[0].decision = 'second';
  assert.equal(validatePlannedCases(value), value);
  assert.deepEqual(secondMatrix(value), { include: [{ suite: 'okx' }] }, 'other suites still remeasure');
  for (const invalid of [{ suite: 'bitget', refused: '' }, { suite: 'bitget', refused: 'x', cases: [] }, { suite: 'unknown', refused: 'x' }]) {
    const bad = plan(); bad.suites[index] = invalid; assert.throws(() => validatePlannedCases(bad));
  }
});

test('a CPU-only miss beyond remeasurement is a planned CPU refusal, never a second runner', () => {
  const value = plan();
  value.suites.find(row => row.suite === 'bybit').cases[0].decision = 'cpu-refused';
  assert.equal(validatePlannedCases(value), value);
  assert.deepEqual(secondMatrix(value), { include: [] });
  assert.equal(requireSecondState(value.suites.find(row => row.suite === 'bybit').cases, []), false);
});

test('only Bitget may plan a non-certified oracle refusal without a second measurement', () => {
  const value = plan(), bitget = value.suites.find(row => row.suite === 'bitget');
  bitget.cases[0].decision = 'not-certified';
  bitget.oracleRefusal = { artifactId: 900, name: 'bitget-paired-steady', digest: 'sha256:' + 'd'.repeat(64), bytes: 10 };
  assert.equal(validatePlannedCases(value), value);
  assert.deepEqual(secondMatrix(value), { include: [] });
  const wrongSuite = structuredClone(value);
  wrongSuite.suites.find(row => row.suite === 'bybit').cases[0].decision = 'not-certified';
  assert.throws(() => validatePlannedCases(wrongSuite));
  assert.equal(requireSecondState(bitget.cases, []), false);
  assert.throws(() => requireSecondState(bitget.cases, [job()]));
  for (const change of [x => { delete x.oracleRefusal; },
    x => { x.oracleRefusal.name = 'other'; }, x => { x.oracleRefusal.digest = 'bad'; }]) {
    const invalid = structuredClone(value); change(invalid.suites.find(row => row.suite === 'bitget'));
    assert.throws(() => validatePlannedCases(invalid));
  }
});

test('a recorded Bitget refusal leaves 31 gates usable but never certifies Bitget', async () => {
  const reason = 'NOT-CERTIFIED: Bitget TS oracle output changed';
  const { results, certificates } = await certifySuites(GATE_SUITES, async suite => {
    if (suite === 'bitget') throw new Error(reason);
    return { suite };
  });
  assert.equal(results.certified, false);
  assert.equal(results.suites.bitget, false);
  assert.equal(certificates.bitget, undefined);
  const complete = { schema: 1, runId: 10, checkoutSha: 'c'.repeat(40), ...results };
  for (const suite of GATE_SUITES) {
    const args = { suite, results: complete, result: 'success', runId: 10, checkoutSha: complete.checkoutSha };
    if (suite === 'bitget') assert.throws(() => requireCertifiedSuite(args), /NOT-CERTIFIED/);
    else assert.equal(requireCertifiedSuite(args), true);
  }
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
    requiredImprovement: .1, failures: ['snapshot fixture mismatch'], correctnessRuns: 1, runs: PUBLIC_REPLAY_PAIRS,
    samples: { ts: Array(PUBLIC_REPLAY_PAIRS).fill(metric(200, 100)), rust: Array(PUBLIC_REPLAY_PAIRS).fill(metric(100, 50)) } };
  const manifest = { schema: 1, failures: [{ venue: 'fixture', tapeSha256: report.tapeSha256,
    diagnostics: [{ key: 'snapshot fixture mismatch', divergence: 'existing declared fixture divergence' }] }] };
  assert.equal(classifyInitialCase({ report }, manifest).decision, 'first');
  assert.equal(report.status, 'FAIL');
  report.failures.push('performance main-thread CPU: fake');
  assert.throws(() => classifyInitialCase({ report }, manifest));
});


test('required check wrappers accept only successful complete same-run same-checkout certification', () => {
  const args = { suite: 'bitgetusdc', result: 'success', runId: 1, checkoutSha: 'a'.repeat(40),
    results: { schema: 1, runId: 1, checkoutSha: 'a'.repeat(40), certified: true, reasons: {}, suites: Object.fromEntries(GATE_SUITES.map(suite => [suite, true])) } };
  assert.equal(requireCertifiedSuite(args), true);
  for (const change of [value => { value.result = 'failure'; }, value => { value.result = 'skipped'; },
    value => { value.results = null; }, value => { value.results.runId = 2; }, value => { value.results.checkoutSha = 'b'.repeat(40); },
    value => { delete value.results.suites.woo; }, value => { value.results.suites.bitgetusdc = false; },
    value => { value.results.suites.woo = 'yes'; }, value => { value.results.certified = false; },
    value => { value.results.suites.unknown = true; }, value => { value.suite = 'unknown'; }]) {
    const bad = structuredClone(args); change(bad); assert.throws(() => requireCertifiedSuite(bad));
  }
});

test('a CPU gate accepts its own certificate while another suite is uncertified, never the reverse', () => {
  const suites = Object.fromEntries(GATE_SUITES.map(suite => [suite, suite !== 'txflow']));
  const reason = 'CPU certification: txflow not certified: runner image rolled 20261026.354.1→20261101.360.1; fresh run required';
  const results = { schema: 1, runId: 1, checkoutSha: 'a'.repeat(40), certified: false, suites, reasons: { txflow: reason } };
  const args = { result: 'success', runId: 1, checkoutSha: 'a'.repeat(40), results };
  assert.equal(requireCertifiedSuite({ ...args, suite: 'bitget' }), true);
  assert.throws(() => requireCertifiedSuite({ ...args, suite: 'txflow' }), (error) => error.message.includes(reason));
  // Overall certification stays false while any suite is missing; a claimed overall pass is refused.
  assert.throws(() => requireCertifiedSuite({ ...args, suite: 'bitget', results: { ...results, certified: true } }));
  assert.throws(() => requireCertifiedSuite({ ...args, suite: 'bitget', results: { ...results, reasons: {} } }));
});

test('certifyAll records each suite and keeps overall certification false when one is refused', async () => {
  const refused = new Error('CPU certification: txflow not certified: runner image rolled a→b; fresh run required');
  const { results, certificates } = await certifySuites(['bitget', 'txflow', 'woo'], async (suite) => {
    if (suite === 'txflow') throw refused;
    return { suite };
  });
  assert.deepEqual(results, { certified: false, suites: { bitget: true, txflow: false, woo: true }, reasons: { txflow: refused.message } });
  assert.deepEqual(Object.keys(certificates), ['bitget', 'woo']);
  const all = await certifySuites(['bitget', 'woo'], async (suite) => ({ suite }));
  assert.deepEqual(all.results, { certified: true, suites: { bitget: true, woo: true }, reasons: {} });
});

const sealed = { ImageOS: 'ubuntu24', ImageVersion: '20261026.354.1', arch: 'x64', nodeVersion: 'v22.1.0', nodeVersions: { node: '22.1.0', v8: 'fixture-v8' } };
const rolled = { ...sealed, ImageVersion: '20261101.360.1' };
const receiptArgs = { suite: 'txflow', expected: sealed, environment: rolled, runId: 41898670964, firstJobId: 126830000000 };

test('run 41898670964: a second runner on a rolled image writes a typed unmeasured receipt', () => {
  assert.equal(imageRollReceipt({ ...receiptArgs, environment: sealed }), null);
  assert.deepEqual(imageRollReceipt(receiptArgs), { schema: 1, suite: 'txflow', status: 'image-rolled', measured: false,
    runId: 41898670964, firstJobId: 126830000000, expected: sealed, actual: rolled,
    differences: [{ field: 'ImageVersion', expected: '20261026.354.1', actual: '20261101.360.1' }] });
  // Every other environment change stays the existing hard refusal.
  for (const environment of [{ ...rolled, arch: 'arm64' }, { ...sealed, nodeVersion: 'v22.2.0', nodeVersions: { node: '22.2.0', v8: 'fixture-v8' } },
    { ...sealed, nodeVersions: { ...sealed.nodeVersions, v8: 'other' } }])
    assert.throws(() => imageRollReceipt({ ...receiptArgs, environment }), /environment identity mismatch/);
});

test('the second job checks image identity before downloading the capsule or restoring anything', () => {
  const source = readFileSync(new URL('./cpu-ci.mjs', import.meta.url), 'utf8');
  const body = source.slice(source.indexOf('async function secondMeasurement('), source.indexOf('\n}\n', source.indexOf('async function secondMeasurement(')));
  const check = body.indexOf('imageRollReceipt(');
  assert.ok(check > 0, 'secondMeasurement checks the runner image');
  for (const later of ["`cpu-runtime-${suite}`", 'artifactEntriesToDirectory(', 'restoreRuntime(', 'spawnSync('])
    assert.ok(body.indexOf(later) > check, `${later} follows the image check`);
  assert.match(body.slice(check, body.indexOf("`cpu-runtime-${suite}`")), /return /, 'a rolled image ends the job before any restore');
});

test('certification turns only an authentic unmeasured image-rolled receipt into a typed refusal', () => {
  const receipt = imageRollReceipt(receiptArgs);
  const entries = (value = receipt, extra = {}) => new Map([['cpu-second/image-rolled.json', Buffer.from(JSON.stringify(value))],
    ...Object.entries(extra).map(([name, value]) => [name, Buffer.from(value)])]);
  const refuse = (overrides = {}) => imageRolledRefusal({ suite: 'txflow', entries: entries(), expected: sealed, logBytes: Buffer.from('no markers'),
    runId: 41898670964, firstJobId: 126830000000, ...overrides });
  assert.equal(refuse(), 'txflow not certified: runner image rolled 20261026.354.1→20261101.360.1; fresh run required');
  assert.equal(refuse({ entries: new Map() }), null);
  for (const overrides of [
    { entries: entries({ ...receipt, measured: true }) },
    { entries: entries(receipt, { 'cpu-second/cases/x.json': '{}' }) },
    { entries: entries(receipt, { 'second.json': '{}' }) },
    { entries: entries(receipt, { 'manifest.json': '{}' }) },
    { entries: entries({ ...receipt, suite: 'woo' }) },
    { entries: entries({ ...receipt, status: 'rolled' }) },
    { entries: entries({ ...receipt, actual: sealed, differences: [] }) },
    { entries: entries({ ...receipt, actual: { ...rolled, arch: 'arm64' } }) },
    { entries: entries({ ...receipt, differences: [] }) },
    { expected: { ...sealed, ImageVersion: 'other' } },
    { runId: 1 },
    { firstJobId: 1 },
    { logBytes: Buffer.from(`\nCPU_CASE_BEGIN ${'a'.repeat(64)} 2026-11-04T19:00:00.000Z`) },
  ]) assert.throws(() => refuse(overrides), /image-rolled receipt/, JSON.stringify(Object.keys(overrides)));
});

test('status names incomplete shared certification before reading absent or malformed output', () => {
  for (const result of ['failure', 'cancelled', 'skipped', '']) for (const output of ['', '{not JSON']) {
    const child = spawnSync(process.execPath, ['sdk-rs/harness/cpu-ci.mjs', '--command', 'status', '--suite', 'bitget'], {
      encoding: 'utf8', env: { ...process.env, CPU_CERT_RESULT: result, CPU_CERT_RESULTS: output },
    });
    assert.equal(child.status, 1);
    assert.equal(child.stderr.trim(), `shared CPU certification did not complete (result=${result || 'missing'}); certification not accepted`);
    assert.doesNotMatch(child.stdout, /accepted/);
  }
});

test('successful upstream status still requires complete same-run same-checkout certification', () => {
  const checkoutSha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const results = { schema: 1, runId: 123, checkoutSha, certified: true, reasons: {}, suites: Object.fromEntries(GATE_SUITES.map(suite => [suite, true])) };
  const status = value => spawnSync(process.execPath, ['sdk-rs/harness/cpu-ci.mjs', '--command', 'status', '--suite', 'bitget'], {
    encoding: 'utf8', env: { ...process.env, CPU_CERT_RESULT: 'success', CPU_CERT_RESULTS: value, GITHUB_RUN_ID: '123' },
  });
  const pass = status(JSON.stringify(results));
  assert.equal(pass.status, 0, pass.stderr);
  assert.match(pass.stdout, /CPU certification accepted for bitget/);
  for (const mutate of [value => { value.runId++; }, value => { value.checkoutSha = 'f'.repeat(40); },
    value => { delete value.suites.woo; }, value => { value.suites.bitget = false; }]) {
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
