import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { setImmediate } from 'node:timers/promises';
import { test } from 'node:test';
import { authenticatedBundle, evidenceEndCheck } from './cpu-ci.mjs';
import { CPU_COHORT_STEPS, createGithubReader } from './cpu-cohort.mjs';

const checkout = 'a'.repeat(40);
const at = second => `2026-10-24T00:00:${String(second).padStart(2, '0')}.000Z`;
const completed = (name, start, end, conclusion = 'success') => ({ name,
  status: 'completed', conclusion, started_at: at(start), completed_at: at(end) });
const pending = name => ({ name, status: 'pending', conclusion: null,
  started_at: null, completed_at: null });

function fixture(observations, { corruptDigest = false, refuseJobs = false,
  extraProducers = [], mutateJobs = () => {} } = {}) {
  const run = { id: 1000, run_number: 1, run_attempt: 1, workflow_id: 386369161,
    path: '.github/workflows/rust.yml', head_sha: checkout, head_branch: 'feat/cpu',
    event: 'workflow_dispatch' };
  const job = { id: 100, run_id: run.id, run_attempt: 1, runner_id: 101,
    head_sha: checkout, head_branch: run.head_branch,
    name: 'Rust / replay measurement (bitget)', status: 'completed', conclusion: 'success',
    started_at: at(0), completed_at: at(29), steps: [
      completed('Run actions/checkout@v7', 1, 3),
      completed('Public steady replay', 5, 15),
      completed(CPU_COHORT_STEPS.strict, 20, 25),
    ] };
  const bytes = Buffer.from('bounded metadata authority control');
  const digest = createHash('sha256').update(bytes).digest('hex');
  const meta = { id: 700, name: 'strict-replay-bitget', expired: false,
    size_in_bytes: bytes.length, digest: `sha256:${corruptDigest ? 'b'.repeat(64) : digest}`,
    created_at: at(22), updated_at: at(23), workflow_run: {
      id: run.id, head_sha: checkout, head_branch: run.head_branch,
    } };
  const log = artifact => Buffer.from(`${at(2)} [command]/usr/bin/git log -1 --format=%H\n${at(2)} ${checkout}\n` +
    `${at(21)} SHA256 digest of uploaded artifact is ${digest}\n` +
    `${at(22)} Artifact ${artifact.name} successfully finalized. Artifact ID ${artifact.id}\n` +
    `${at(23)} Artifact ${artifact.name} has been successfully uploaded! Final size is ${artifact.size_in_bytes} bytes. Artifact ID is ${artifact.id}\n`);
  const jobs = observations.map(mutate => { const value = structuredClone(job); mutate(value); return value; });
  const others = extraProducers.map((suite, index) => ({ ...structuredClone(job),
    id: job.id + index + 1, runner_id: job.runner_id + index + 1,
    name: `Rust / replay measurement (${suite})` }));
  const artifacts = [meta, ...extraProducers.map((suite, index) => ({ ...structuredClone(meta),
    id: meta.id + index + 1, name: `strict-replay-${suite}` }))];
  const calls = { jobs: 0, logs: 0, downloads: 0 };
  const context = { runId: run.id, bindings: [], reader: {
    getRun: async () => structuredClone(run),
    listJobs: async () => {
      calls.jobs++;
      if (refuseJobs) throw new Error('GitHub evidence unavailable (403)');
      const result = [structuredClone(jobs[Math.min(calls.jobs - 1, jobs.length - 1)]), ...structuredClone(others)];
      mutateJobs(result, calls.jobs - 1);
      return result;
    },
    listArtifacts: async () => structuredClone(artifacts),
    getLog: async id => {
      const index = [job, ...others].findIndex(value => value.id === id);
      assert.ok(index >= 0, 'unknown physical producer'); calls.logs++;
      return log(artifacts[index]);
    },
    getArtifactBytes: async () => { calls.downloads++; throw new Error('unexpected ZIP read in metadata-only control'); },
  } };
  const readSuite = suite => authenticatedBundle(context, run.id, `strict-replay-${suite}`,
    `Rust / replay measurement (${suite})`, CPU_COHORT_STEPS.strict, checkout, true);
  const read = () => readSuite('bitget');
  return { read, readSuite, context, calls, jobs, run };
}

// Advance only virtual timer waits; no real GitHub requests or wall-clock sleep.
async function settle(t, read) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const outcome = read().then(value => ({ value }), error => ({ error }));
  for (let round = 0; round < 12; round++) {
    await setImmediate();
    t.mock.timers.tick(5_000);
  }
  return outcome;
}

test('original: completed producer with pending upload waits for the same terminal physical record', async t => {
  const value = fixture([job => { job.steps[2] = pending(CPU_COHORT_STEPS.strict); }, () => {}]);
  const result = await settle(t, value.read);
  assert.ifError(result.error);
  assert.equal(value.calls.jobs, 2, 'must refresh the stale producer inventory');
  assert.deepEqual(result.value.job, value.jobs[1]);
  assert.deepEqual(value.context.bindings[0].job, value.jobs[1]);
  assert.equal(result.value.receipt.attempt, 1);
});

test('original: successful upload does not bind another nonterminal producer step', async t => {
  const value = fixture([job => { job.steps[1] = { ...job.steps[1], status: 'in_progress', conclusion: null, completed_at: null }; }, () => {}]);
  const result = await settle(t, value.read);
  assert.ifError(result.error);
  assert.equal(value.calls.jobs, 2, 'all producer steps must settle before binding');
  assert.deepEqual(result.value.job, value.jobs[1]);
  assert.deepEqual(value.context.bindings[0].job, value.jobs[1]);
});

test('original: never-terminal producer exhausts the bounded metadata wait without acceptance', async t => {
  const value = fixture([job => { job.steps[1] = pending(job.steps[1].name); }]);
  const result = await settle(t, value.read);
  assert.ok(result.error, 'must reject a producer whose steps never become terminal');
  assert.match(result.error.message, /producer steps.*terminal.*6.*refresh/i);
  assert.match(result.error.message, /run 1000/);
  assert.match(result.error.message, /job 100 Rust \/ replay measurement \(bitget\)/);
  assert.match(result.error.message, /Public steady replay \[pending\/null\]/);
  assert.equal(value.calls.jobs, 7, 'initial inventory plus at most six shared refreshes');
  assert.equal(value.context.bindings.length, 0);
  assert.equal(value.context.bundles.size, 0);
});

test('working control: terminal original producer binds without a metadata poll', async () => {
  const value = fixture([() => {}]);
  const result = await value.read();
  assert.equal(result.receipt.attempt, 1);
  assert.equal(value.calls.jobs, 1);
  assert.equal(value.context.bindings.length, 1);
  assert.equal(value.calls.downloads, 0);
});

for (const [name, mutate, expected] of [
  ['failed upload', job => { job.steps[2].conclusion = 'failure'; }, /missing unique successful step/],
  ['attempt2 producer', job => { job.run_attempt = 2; }, /attempt1|physical/],
  ['duplicate upload step', job => { job.steps.push(structuredClone(job.steps[2])); }, /missing unique successful step/],
]) test(`working control: ${name} is refused`, async () => {
  const value = fixture([mutate]);
  await assert.rejects(value.read, expected);
  assert.equal(value.context.bindings.length, 0);
  assert.equal(value.context.bundles.size, 0);
  assert.equal(value.calls.jobs, 1);
});

test('working control: corrupt upload digest is still refused', async () => {
  const value = fixture([() => {}], { corruptDigest: true });
  await assert.rejects(value.read, /upload digest belongs to different bytes/);
  assert.equal(value.context.bindings.length, 0);
  assert.equal(value.context.bundles.size, 0);
});

test('working control: HTTP refusal is not a metadata convergence retry', async () => {
  const value = fixture([() => {}], { refuseJobs: true });
  await assert.rejects(value.read, /GitHub evidence unavailable \(403\)/);
  assert.equal(value.calls.jobs, 1);
  assert.equal(value.context.bindings.length, 0);
});

for (const [name, change] of [
  ['attempt2 alias', rows => { rows.push({ ...structuredClone(rows[0]), id: 999, run_attempt: 2 }); }],
  ['duplicate producer', rows => { rows.push({ ...structuredClone(rows[0]), id: 999 }); }],
  ['changed job id', rows => { rows[0].id++; }],
  ['changed runner', rows => { rows[0].runner_id++; }],
]) test(`refresh control: ${name} cannot replace the original producer`, async t => {
  const value = fixture([job => { job.steps[2] = pending(CPU_COHORT_STEPS.strict); }, () => {}], {
    mutateJobs: (rows, read) => { if (read > 0) change(rows); },
  });
  const result = await settle(t, value.read);
  assert.match(result.error?.message ?? '', /unique.*physical.*attempt1|physical identity changed/);
  assert.equal(value.calls.jobs, 2);
  assert.equal(value.context.bindings.length, 0);
  assert.equal(value.context.bundles.size, 0);
});

test('refresh control: two lagging producers share six refreshes rather than twelve', async t => {
  const value = fixture([() => {}], { extraProducers: ['bybit'], mutateJobs: (rows, read) => {
    if (read < 3) rows[0].steps[2] = pending(CPU_COHORT_STEPS.strict);
    rows[1].steps[2] = pending(CPU_COHORT_STEPS.strict);
  } });
  const result = await settle(t, async () => { await value.read(); return value.readSuite('bybit'); });
  assert.match(result.error?.message ?? '', /producer steps not terminal after 6 refreshes: run 1000, job 101/);
  assert.match(result.error.message, /Retain strict gate reports and artifact budgets/);
  assert.equal(value.calls.jobs, 7);
  assert.equal(value.context.producerStepRefreshes, 6);
  assert.equal(value.context.bindings.length, 1, 'only the first terminal producer was bound');
  assert.equal(value.context.bundles.size, 1);
});

test('refresh control: concurrent bindings cannot overspend the shared refresh budget', async t => {
  const suites = ['bitget', 'bybit', 'binance', 'woo', 'okx', 'backpack', 'weex'];
  const value = fixture([() => {}], { extraProducers: suites.slice(1), mutateJobs: (rows, read) => {
    if (read === 0) for (const row of rows) row.steps[2] = pending(CPU_COHORT_STEPS.strict);
  } });
  const result = await settle(t, () => Promise.allSettled(suites.map(value.readSuite)));
  assert.ifError(result.error);
  assert.equal(result.value.filter(row => row.status === 'fulfilled').length, 6);
  assert.equal(result.value.filter(row => row.status === 'rejected').length, 1);
  assert.equal(value.context.producerStepRefreshes, 6);
  assert.equal(value.calls.jobs, 7);
});

test('refresh control: descriptive fields may settle without changing physical identity', async t => {
  const value = fixture([job => {
    job.steps[2] = pending(CPU_COHORT_STEPS.strict); job.completed_at = null; job.labels = ['old'];
  }, job => { job.labels = ['settled']; }]);
  const result = await settle(t, value.read);
  assert.ifError(result.error);
  assert.equal(value.calls.jobs, 2);
  assert.deepEqual(result.value.job, value.jobs[1]);
});

test('working control: active planner and certifier jobs are never awaited as producers', async () => {
  const value = fixture([() => {}], { mutateJobs: rows => {
    for (const [index, name] of ['Rust / CPU remeasurement plan', 'Rust / shared CPU certification'].entries())
      rows.push({ ...structuredClone(rows[0]), id: 900 + index, name,
        status: 'in_progress', conclusion: null, steps: [pending('Active orchestration')] });
  } });
  await value.read();
  assert.equal(value.calls.jobs, 1);
  assert.equal(value.context.producerStepRefreshes ?? 0, 0);
  assert.equal(value.context.bindings.length, 1);
  assert.equal(value.context.bindings[0].job.id, 100);
});

test('refresh control: an earlier bound bundle stays immutable and the final check rejects later drift', async t => {
  const value = fixture([() => {}], { extraProducers: ['bybit'], mutateJobs: (rows, read) => {
    if (read === 0) rows[1].steps[2] = pending(CPU_COHORT_STEPS.strict);
    else rows[0].labels = ['changed after binding'];
  } });
  let original;
  const result = await settle(t, async () => {
    original = await value.read();
    await value.readSuite('bybit');
    return value.read();
  });
  assert.ifError(result.error);
  assert.equal(result.value, original, 'cached bundle must not be rebound');
  assert.equal(value.calls.jobs, 2);
  assert.equal(value.context.bindings.length, 2);
  assert.deepEqual(value.context.bindings[0].job, original.job);
  assert.equal(original.job.labels, undefined);
  await assert.rejects(evidenceEndCheck(value.context), /physical producer became duplicate\/changed/);
});

test('refresh control: completed/null waits, while an unrelated skipped step is terminal', async t => {
  const value = fixture([job => { job.steps[2].conclusion = null; }, job => {
    job.steps.push(completed('Optional cache', 26, 27, 'skipped'));
  }]);
  const result = await settle(t, value.read);
  assert.ifError(result.error);
  assert.equal(value.calls.jobs, 2);
});

test('refresh control: all distinct raw API observations remain retained', async t => {
  const value = fixture([job => { job.steps[2] = pending(CPU_COHORT_STEPS.strict); }, () => {}]);
  const upstream = value.context.reader;
  const reader = createGithubReader({ repository: 'org/repo', fetch: async url => {
    const path = new URL(url).pathname + new URL(url).search;
    if (path.endsWith('/actions/runs/1000')) return Response.json(await upstream.getRun());
    if (path.includes('/jobs?')) return Response.json({ total_count: 1, jobs: await upstream.listJobs() });
    if (path.includes('/artifacts?')) return Response.json({ total_count: 1, artifacts: await upstream.listArtifacts() });
    if (path.endsWith('/actions/jobs/100/logs')) return new Response(await upstream.getLog(100));
    throw new Error(`unexpected evidence request ${path}`);
  } });
  value.context.reader = reader;
  const result = await settle(t, value.read);
  assert.ifError(result.error);
  const observations = reader.snapshots().responses.filter(row => row.path.includes('/jobs?'));
  assert.equal(observations.length, 2);
  assert.notEqual(observations[0].sha256, observations[1].sha256);
  assert.equal(JSON.parse(observations[0].raw).jobs[0].steps[2].status, 'pending');
  assert.equal(JSON.parse(observations[1].raw).jobs[0].steps[2].conclusion, 'success');
});
