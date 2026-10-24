import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { setImmediate } from 'node:timers/promises';
import { test } from 'node:test';
import { authenticatedBundle } from './cpu-ci.mjs';
import { CPU_COHORT_STEPS } from './cpu-cohort.mjs';

const checkout = 'a'.repeat(40);
const at = second => `2026-10-24T00:00:${String(second).padStart(2, '0')}.000Z`;
const completed = (name, start, end, conclusion = 'success') => ({ name,
  status: 'completed', conclusion, started_at: at(start), completed_at: at(end) });
const pending = name => ({ name, status: 'pending', conclusion: null,
  started_at: null, completed_at: null });

function fixture(observations, { corruptDigest = false, refuseJobs = false } = {}) {
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
  const log = Buffer.from(`${at(2)} [command]/usr/bin/git log -1 --format=%H\n${at(2)} ${checkout}\n` +
    `${at(21)} SHA256 digest of uploaded artifact is ${digest}\n` +
    `${at(22)} Artifact ${meta.name} successfully finalized. Artifact ID ${meta.id}\n` +
    `${at(23)} Artifact ${meta.name} has been successfully uploaded! Final size is ${meta.size_in_bytes} bytes. Artifact ID is ${meta.id}\n`);
  const jobs = observations.map(mutate => { const value = structuredClone(job); mutate(value); return value; });
  const calls = { jobs: 0, logs: 0, downloads: 0 };
  const context = { runId: run.id, bindings: [], reader: {
    getRun: async () => structuredClone(run),
    listJobs: async () => {
      calls.jobs++;
      if (refuseJobs) throw new Error('GitHub evidence unavailable (403)');
      return [structuredClone(jobs[Math.min(calls.jobs - 1, jobs.length - 1)])];
    },
    listArtifacts: async () => [structuredClone(meta)],
    getLog: async id => { assert.equal(id, job.id); calls.logs++; return Buffer.from(log); },
    getArtifactBytes: async () => { calls.downloads++; throw new Error('unexpected ZIP read in metadata-only control'); },
  } };
  const read = () => authenticatedBundle(context, run.id, meta.name, job.name,
    CPU_COHORT_STEPS.strict, checkout, true);
  return { read, context, calls, jobs };
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
