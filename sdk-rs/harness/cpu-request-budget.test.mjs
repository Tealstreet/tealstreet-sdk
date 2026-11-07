import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { GATE_SUITES } from './ci-gate-suites.mjs';
import { createGithubReader, CPU_COHORT_STEPS } from './cpu-cohort.mjs';
import { authenticatedBundle, currentPhysical, evidenceEndCheck } from './cpu-ci.mjs';

// Measured for the 31-suite cohort without second runners or the retired venue.
const REQUESTS = 262;
const sha = value => createHash('sha256').update(value).digest('hex');
const at = second => new Date(Date.UTC(2026, 9, 23, 0, 0, second)).toISOString();
function cohort() {
  // Empty, valid ZIP is sufficient for upload authentication/request accounting.
  const archive = Buffer.alloc(22); archive.writeUInt32LE(0x06054b50);
  const run = { id: 100, run_number: 1, run_attempt: 1, workflow_id: 386369161, path: '.github/workflows/rust.yml',
    head_sha: 'a'.repeat(40), head_branch: 'fixture', event: 'push' };
  const jobs = [], artifacts = [], logs = new Map(), counters = { requests: 0 };
  function producer(name, uploads) {
    const job = { id: jobs.length + 1, run_id: run.id, run_attempt: 1, head_sha: run.head_sha, head_branch: run.head_branch,
      runner_id: jobs.length + 1000, status: 'completed', conclusion: 'success', name, started_at: at(0), completed_at: at(100),
      steps: [{ name: 'Run actions/checkout@v7', status: 'completed', conclusion: 'success', started_at: at(1), completed_at: at(3) }] };
    let log = `${at(2)} [command]/usr/bin/git log -1 --format=%H\n${at(2)} ${run.head_sha}\n`;
    for (const [index, [artifactName, stepName]] of uploads.entries()) {
      const start = 10 + index * 10;
      const artifact = { id: artifacts.length + 1, name: artifactName, expired: false, digest: `sha256:${sha(archive)}`,
        size_in_bytes: archive.length, created_at: at(start + 2), updated_at: at(start + 3),
        workflow_run: { id: run.id, head_sha: run.head_sha, head_branch: run.head_branch } };
      job.steps.push({ name: stepName, status: 'completed', conclusion: 'success', started_at: at(start), completed_at: at(start + 5) });
      log += `${at(start + 1)} SHA256 digest of uploaded artifact is ${sha(archive)}\n` +
        `${at(start + 2)} Artifact ${artifactName} successfully finalized. Artifact ID ${artifact.id}\n` +
        `${at(start + 3)} Artifact ${artifactName} has been successfully uploaded! Final size is ${archive.length} bytes. Artifact ID is ${artifact.id}\n`;
      artifacts.push(artifact);
    }
    jobs.push(job); logs.set(job.id, Buffer.from(log));
  }
  for (const suite of GATE_SUITES) producer(`Rust / replay measurement (${suite})`, [
    [`strict-replay-${suite}`, CPU_COHORT_STEPS.strict], [`cpu-cohort-${suite}`, CPU_COHORT_STEPS.first] ]);
  producer('Rust / CPU remeasurement plan', [['cpu-plan', CPU_COHORT_STEPS.plan]]);
  // Include all final wrappers, native/host checks and matching package uploads
  // so both original and fresh inventories cross the 100-row page boundary.
  for (let index = 0; index < 50; index++) jobs.push({ id: jobs.length + 1, name: `unrelated-${index}` });
  for (const suite of GATE_SUITES) artifacts.push({ id: artifacts.length + 1, name: `ci-wasm-${suite}` });
  const context = () => ({ runId: run.id, checkoutSha: run.head_sha, bindings: [], reader: createGithubReader({ repository: 'org/repo', fetch: async url => {
    counters.requests++;
    const value = new URL(url), path = value.pathname.replace('/repos/org/repo/', '');
    if (path === `actions/runs/${run.id}`) return Response.json(run);
    for (const [kind, rows] of [['jobs', jobs], ['artifacts', artifacts]]) if (path === `actions/runs/${run.id}/${kind}`) {
      const start = (Number(value.searchParams.get('page')) - 1) * 100;
      return Response.json({ total_count: rows.length, [kind]: rows.slice(start, start + 100) });
    }
    const log = /^actions\/jobs\/(\d+)\/logs$/.exec(path);
    if (log) return new Response(logs.get(Number(log[1])));
    if (/^actions\/artifacts\/\d+\/zip$/.test(path)) return new Response(archive);
    throw new Error(`Unexpected unbudgeted API request ${path}`);
  } }) });
  const bundle = (ctx, name, jobName, stepName) => authenticatedBundle(ctx, run.id, name, jobName, stepName, run.head_sha);
  const first = async (ctx, suite) => {
    for (const [name, step] of [[`strict-replay-${suite}`, CPU_COHORT_STEPS.strict], [`cpu-cohort-${suite}`, CPU_COHORT_STEPS.first]])
      await bundle(ctx, name, `Rust / replay measurement (${suite})`, step);
  };
  const plan = ctx => bundle(ctx, 'cpu-plan', 'Rust / CPU remeasurement plan', CPU_COHORT_STEPS.plan);
  return { context, bundle, first, plan, counters, jobs, artifacts, run };
}

async function exercise() {
  const value = cohort(), { context, first, plan, counters } = value;
  for (const suite of GATE_SUITES) await currentPhysical(context(), `Rust / replay measurement (${suite})`);
  const planner = context();
  for (const suite of GATE_SUITES) await first(planner, suite);
  await evidenceEndCheck(planner);
  const certifier = context(); await plan(certifier);
  for (const suite of GATE_SUITES) {
    await first(certifier, suite); await first(certifier, suite); // repeated source recomputation shares immutable bundles
  }
  await evidenceEndCheck(certifier);
  return { ...value, certifier, requests: counters.requests };
}

test('actual paginated authority adapter stays bounded for a complete cohort', async () => {
  const cohortRun = await exercise();
  assert.equal(cohortRun.requests, REQUESTS);
  assert(cohortRun.requests < 1000, 'one complete cohort must fit the ordinary repository token budget before concurrent activity');
  assert.equal(cohortRun.certifier.bindings.length, 1 + 2 * GATE_SUITES.length);
});

test('batching still freshly rejects late duplicate artifacts/jobs and run-attempt drift', async () => {
  for (const mutation of [value => value.artifacts.push({ ...value.artifacts[0], id: 9999 }),
    value => value.jobs.push({ ...value.jobs[0], id: 9999 }), value => { value.run.run_attempt = 2; }]) {
    const value = await exercise(); mutation(value);
    await assert.rejects(evidenceEndCheck(value.certifier), /duplicate\/changed|attempt changed/);
  }
});
