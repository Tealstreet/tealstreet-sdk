import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { CPU_COHORT_STEPS, sourceInputFingerprint, cohortKey, parseCohortJson, createGithubReader,
  authenticateArtifactExecution, authenticateMeasurementWindow, formatMeasurementMarker, requireGithubResponseSize } from './cpu-cohort.mjs';
const anchor = { workflowId: 386369161, runNumber: 1, runId: 1000 };
const hash = (value) => createHash('sha256').update(value).digest('hex');
const git = (value) => createHash('sha1').update(value).digest('hex');
const gitBlob = (value) => git(Buffer.concat([Buffer.from(`blob ${value.length}\0`), value]));
const at = (second) => `2026-10-23T00:00:${String(second).padStart(2, '0')}.000Z`;
const caseValue = { venue: 'bitget', tape: 'tapes/bitget-public.jsonl', tapeSha256: hash('tape'), books: 'BTCUSDT', private: false, correctnessRuns: 1, runs: 9 };
const clone = (value) => structuredClone(value);
const policy = Buffer.from('export const rule = "all-objective-normalized-two-run-mean-v1";');
const row = (path, value) => ({ path, type: 'blob', mode: '100644', sha: gitBlob(Buffer.from(value)) });
const entries = [row('sdk-rs/harness/cpu-remeasure.mjs', policy), row('sdk-rs/harness/gate.mjs', 'runtime')];
const tree = { sha: git('tree'), truncated: false, tree: entries };
const policyBlob = { sha: gitBlob(policy), encoding: 'base64', content: policy.toString('base64') };
const inputsSha256 = sourceInputFingerprint(entries), policySha256 = hash(policy);
function run(number, extra = {}) {
  return { id: anchor.runId + number - anchor.runNumber, run_number: number, run_attempt: 1, workflow_id: anchor.workflowId, path: '.github/workflows/rust.yml', head_sha: git(`head${number}`), head_branch: 'feat/cpu', event: 'pull_request', ...extra };
}
function job(runValue, { second = false, id = 100, runner = 101, legacy = false } = {}) {
  const start = second ? 30 : 0, stop = second ? 59 : 29;
  return { id, run_id: runValue.id, run_attempt: 1, head_sha: runValue.head_sha, head_branch: runValue.head_branch,
    name: `Rust / ${second ? 'CPU remeasurement' : legacy ? 'replay gate' : 'replay measurement'} (bitget)`, runner_id: runner,
    status: 'completed', conclusion: 'success', started_at: at(start), completed_at: at(stop),
    steps: [{ name: 'Run actions/checkout@v7', status: 'completed', conclusion: 'success', started_at: at(start + 1), completed_at: at(start + 3) },
      { name: second ? CPU_COHORT_STEPS.second : CPU_COHORT_STEPS.first, status: 'completed', conclusion: 'success', started_at: at(start + 20), completed_at: at(start + 25) }] };
}
function artifact(runValue, second = false) {
  const raw = Buffer.from(second ? 'second immutable ZIP fixture' : 'first immutable ZIP fixture');
  return { raw, meta: { id: second ? 701 : 700, name: `cpu-${second ? 'second' : 'cohort'}-bitget`, expired: false, size_in_bytes: raw.length, digest: `sha256:${hash(raw)}`, created_at: at(second ? 52 : 22), updated_at: at(second ? 53 : 23), workflow_run: { id: runValue.id, head_sha: runValue.head_sha, head_branch: runValue.head_branch } } };
}
function log(checkout, meta, second = false) {
  const start = second ? 30 : 0;
  return Buffer.from(`${at(start + 2)} [command]/usr/bin/git log -1 --format=%H\n${at(start + 2)} ${checkout}\n${at(start + 21)} SHA256 digest of uploaded artifact is ${meta.digest.slice(7)}\n${at(start + 22)} Artifact ${meta.name} successfully finalized. Artifact ID ${meta.id}\n${at(start + 23)} Artifact ${meta.name} has been successfully uploaded! Final size is ${meta.size_in_bytes} bytes. Artifact ID is ${meta.id}\n`);
}
function fixture({ second = false, prior = false, phase = second ? 'final' : 'plan' } = {}) {
  const currentRun = run(anchor.runNumber + Number(prior)), checkout = git(`merge${currentRun.id}`), first = job(currentRun), firstArtifact = artifact(currentRun);
  const observation = { job: first, logBytes: log(checkout, firstArtifact.meta), checkout: { sha: checkout, tree: { sha: tree.sha }, parents: [{ sha: git('base') }, { sha: currentRun.head_sha }] }, tree: clone(tree), policyBlob: clone(policyBlob) };
  const current = { ...currentRun, suite: 'bitget', case: clone(caseValue), inputsSha256, policySha256, phase };
  const claim = { schema: 1, suite: current.suite, case: clone(caseValue), key: cohortKey(current), inputsSha256, policySha256, measuredCheckoutSha: checkout, runId: currentRun.id, runNumber: currentRun.run_number, attempt: 1, firstJobId: first.id, runtimeSha256: hash('runtime capsule') };
  const record = { runId: currentRun.id, jobs: { complete: true, items: [first] }, observations: [observation], artifact: firstArtifact.meta, artifactBytes: firstArtifact.raw, claim };
  const runs = [currentRun], claims = [record];
  if (second) {
    const next = job(currentRun, { second: true, id: 102, runner: 202 }), nextArtifact = artifact(currentRun, true);
    record.jobs.items.push(next); Object.assign(record, { secondArtifact: nextArtifact.meta, secondArtifactBytes: nextArtifact.raw, secondLogBytes: log(checkout, nextArtifact.meta, true), secondClaim: { schema: 1, suite: 'bitget', claims: [{ ...clone(claim), jobId: next.id }] } });
  }
  if (prior) {
    const previous = run(anchor.runNumber), previousJob = job(previous, { id: 50, legacy: true }), previousCheckout = git('pre-policy merge');
    runs.unshift(previous); const preTree = { sha: git('oldtree'), truncated: false, tree: [row('sdk-rs/harness/gate.mjs', 'runtime')] };
    claims.unshift({ runId: previous.id, jobs: { complete: true, items: [previousJob] }, observations: [{ job: previousJob, logBytes: log(previousCheckout, firstArtifact.meta), checkout: { sha: previousCheckout, tree: { sha: preTree.sha }, parents: [{ sha: git('oldbase') }, { sha: previous.head_sha }] }, tree: preTree, policyBlob: null }] });
  }
  return { anchor, current, runs, claims };
}
const jsonResponse = value => new Response(JSON.stringify(value), { status: 200 });
function apiFixture(value, mutate = () => {}) {
  const calls = [], record = value.claims.at(-1), runValue = value.runs.at(-1), source = record.observations[0];
  const artifacts = [record.artifact, ...(record.secondArtifact ? [record.secondArtifact] : [])];
  const fetch = async (url, options) => {
    calls.push({ url, options }); const path = new URL(url).pathname.replace('/repos/org/repo/', '') + new URL(url).search;
    let result;
    if (path === `actions/runs/${runValue.id}`) result = runValue;
    else if (path === `actions/workflows/${anchor.workflowId}/runs?per_page=100&page=1`) result = { total_count: value.runs.length, workflow_runs: [...value.runs].reverse() };
    else if (path === `actions/runs/${runValue.id}/jobs?filter=all&per_page=100&page=1`) result = { total_count: record.jobs.items.length, jobs: record.jobs.items };
    else if (path === `actions/runs/${runValue.id}/artifacts?per_page=100&page=1`) result = { total_count: artifacts.length, artifacts };
    else if (artifacts.some(item => path === `actions/artifacts/${item.id}`)) result = artifacts.find(item => path === `actions/artifacts/${item.id}`);
    else if (path === `actions/artifacts/${record.artifact.id}/zip`) return new Response(record.artifactBytes);
    else if (record.secondArtifact && path === `actions/artifacts/${record.secondArtifact.id}/zip`) return new Response(record.secondArtifactBytes);
    else if (path === `actions/jobs/${source.job.id}/logs`) return new Response(source.logBytes);
    else if (record.secondArtifact && path === `actions/jobs/${record.secondClaim.claims[0].jobId}/logs`) return new Response(record.secondLogBytes);
    else if (path === `git/commits/${source.checkout.sha}`) result = source.checkout;
    else if (path === `git/trees/${source.tree.sha}?recursive=1`) result = source.tree;
    else if (path === `git/blobs/${source.policyBlob.sha}`) result = source.policyBlob;
    else throw new Error(`Unexpected fixture request ${path}`);
    const copy = clone(result); mutate(path, copy, calls); return jsonResponse(copy);
  };
  return { calls, fetch };
}

test('input and key identity ignores provenance/docs/order, binds runtime/modes/public case', () => {
  const original = sourceInputFingerprint(entries);
  assert.notEqual(sourceInputFingerprint([...entries, row('apps/cli/bin/tealstreet.cjs', 'CLI entry')]), original);
  assert.equal(sourceInputFingerprint([...entries].reverse().concat(row('docs/plan.md', 'new docs'))), original);
  assert.equal(cohortKey({ policySha256, inputsSha256, suite: 'bitget', case: caseValue, runId: 1, artifactId: 7 }), cohortKey({ policySha256, inputsSha256, suite: 'bitget', case: caseValue, runId: 999 }));
  for (const change of [rows => { rows[1].sha = git('changed runtime'); }, rows => { rows[1].mode = '100755'; }]) { const rows = clone(entries); change(rows); assert.notEqual(sourceInputFingerprint(rows), original); }
  assert.notEqual(cohortKey({ policySha256, inputsSha256, suite: 'bitget', case: { ...caseValue, books: 'ETHUSDT' } }), cohortKey({ policySha256, inputsSha256, suite: 'bitget', case: caseValue }));
  for (const rows of [[], [...entries, entries[0]], [row('../gate.mjs', 'x')], [{ ...entries[0], oid: git('foreign') }]]) assert.throws(() => sourceInputFingerprint(rows));
  for (const delta of [{ private: true }, { runs: 7 }, { unexpected: true }]) assert.throws(() => cohortKey({ policySha256, inputsSha256, suite: 'bitget', case: { ...caseValue, ...delta } }));
});

test('strict JSON retains duplicate/prototype/nonfinite/trailing/invalid UTF8 defenses', () => {
  assert.deepEqual(parseCohortJson('{"a":[1,true,null,"x"]}'), { a: [1, true, null, 'x'] });
  assert.equal(Object.hasOwn(parseCohortJson('{"__proto__":1}'), '__proto__'), true);
  for (const raw of ['{"a":1,"a":2}', '{"x":{"k":1,"k":2}}', '1e999', '{}{}', '[1,]', '{"a":1,}', '"bad\nstring"', Buffer.from([0xff])]) assert.throws(() => parseCohortJson(raw));
});

test('complete pagination refuses gaps, duplicate IDs, status errors and untrusted redirects', async () => {
  for (const data of [{ total_count: 2, jobs: [{ id: 1 }] }, { total_count: 2, jobs: [{ id: 1 }, { id: 1 }] }]) {
    const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => jsonResponse(data) });
    await assert.rejects(reader.listJobs(1));
  }
  const badStatus = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => new Response('missing', { status: 404 }) });
  await assert.rejects(badStatus.getRun(1));
  const redirect = createGithubReader({ sleep: async () => {}, repository: 'org/repo', token: 'secret', fetch: async () => new Response(null, { status: 302, headers: { location: 'https://evil.example/bytes' } }) });
  await assert.rejects(redirect.getLog(1), /untrusted/);
});

test('all-attempt pagination follows every page without latest-only selection', async () => {
  const calls = [], jobs = Array.from({ length: 101 }, (_, index) => ({ id: index + 1 }));
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async url => {
    calls.push(url); const page = Number(new URL(url).searchParams.get('page'));
    return jsonResponse({ total_count: 101, jobs: page === 1 ? jobs.slice(0, 100) : jobs.slice(100) });
  } });
  assert.deepEqual(await reader.listJobs(10), jobs);
  assert.equal(calls.length, 2); assert.ok(calls.every(url => new URL(url).searchParams.get('filter') === 'all'));
});

test('artifact CDN receives no token; immutable original bytes retained', async () => {
  const calls = [], raw = Buffer.from('\ufefforiginal log\r\n');
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', token: 'secret', fetch: async (url, options) => { calls.push({ url, options }); return calls.length === 1 ? new Response(null, { status: 302, headers: { location: 'https://fixture.blob.core.windows.net/log' } }) : new Response(raw); } });
  assert.deepEqual(await reader.getLog(10), raw);
  assert.equal(calls[1].options.headers, undefined); assert.equal(calls[1].options.redirect, 'error');
  assert.deepEqual(reader.snapshots().logs[0].raw, raw);
});

const socketClosure = () => new TypeError('fetch failed', { cause: { code: 'UND_ERR_SOCKET' } });

test('immutable binary read recovers one socket closure and caches only complete bytes', async () => {
  for (const failBody of [false, true]) {
    const calls = [], raw = Buffer.from('original immutable evidence');
    const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async (url, options) => {
      calls.push({ url, options });
      if (calls.length === 1) {
        if (!failBody) throw socketClosure();
        return { status: 200, arrayBuffer: async () => { throw socketClosure(); } };
      }
      return new Response(raw);
    } });
    assert.deepEqual(await reader.getArtifactBytes(700), raw);
    assert.deepEqual(await reader.getArtifactBytes(700), raw);
    assert.equal(calls.length, 2);
    assert.deepEqual(calls[0], calls[1]);
    assert.equal(reader.snapshots().responses.length, 1);
    assert.deepEqual(reader.snapshots().responses[0].raw, raw);
  }
});

test('transport retry fails closed after three attempts and never retries HTTP4xx', async () => {
  let calls = 0;
  const permanent = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => { calls++; throw socketClosure(); } });
  await assert.rejects(permanent.getArtifactBytes(700), /fetch failed/);
  assert.equal(calls, 3);
  assert.equal(permanent.snapshots().responses.length, 0);
  for (const status of [403, 404, 429]) {
    calls = 0;
    const refused = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => { calls++; return new Response('refused', { status }); } });
    await assert.rejects(refused.getArtifactBytes(700), error => error.message === `CPU cohort: GitHub evidence unavailable (${status})`);
    assert.equal(calls, 1);
  }
});

test('metadata and active-job log transport failures share the fixed three-attempt cap', async () => {
  const calls = [];
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async url => {
    calls.push(url);
    if (url.endsWith('actions/jobs/5')) return jsonResponse({ id: 5, status: 'in_progress', conclusion: null });
    throw socketClosure();
  } });
  await assert.rejects(reader.getRun(1000), /fetch failed/);
  assert.equal(calls.length, 3);
  await reader.getJob(5);
  await assert.rejects(reader.getLog(5), /fetch failed/);
  assert.equal(calls.filter(url => url.endsWith('actions/jobs/5/logs')).length, 3);
});

test('completed physical logs recover transport failures and cache exact bytes', async () => {
  let logCalls = 0;
  const raw = Buffer.from('original completed physical log');
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async url => {
    if (url.endsWith('actions/jobs/5')) return jsonResponse({ id: 5, status: 'completed', conclusion: 'success', completed_at: at(29) });
    logCalls++;
    if (logCalls === 1) throw socketClosure();
    return new Response(raw);
  } });
  await reader.getJob(5);
  assert.deepEqual(await reader.getLog(5), raw);
  assert.deepEqual(await reader.getLog(5), raw);
  assert.equal(logCalls, 2);
  for (const failure of [new TypeError('different failure', { cause: { code: 'OTHER' } }), new Error('different failure', { cause: { code: 'UND_ERR_SOCKET' } })]) {
    let calls = 0;
    const otherError = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => { calls++; throw failure; } });
    await assert.rejects(otherError.getArtifactBytes(700), /different failure/);
    assert.equal(calls, 1);
  }
});

test('immutable CDN recovery repeats the same authority and never forwards its credential', async () => {
  const calls = [], raw = Buffer.from('original ZIP');
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', token: 'fixture-token', fetch: async (url, options) => {
    calls.push({ url, options });
    if (url.startsWith('https://api.github.com/')) return new Response(null, { status: 302, headers: { location: 'https://fixture.blob.core.windows.net/archive' } });
    if (calls.length === 2) throw socketClosure();
    return new Response(raw);
  } });
  assert.deepEqual(await reader.getArtifactBytes(700), raw);
  assert.equal(calls.length, 4);
  assert.deepEqual(calls[0], calls[2]);
  for (const index of [1, 3]) {
    assert.equal(calls[index].options.headers, undefined);
    assert.equal(calls[index].options.redirect, 'error');
  }
  assert.equal(reader.snapshots().responses.length, 1);
});

test('artifact execution wrapper fixes first/second/plan/final job and upload name pairs', () => {
  for (const [jobName, artifactName, stepName] of [
    ['Rust / replay measurement (bitget)', 'cpu-cohort-bitget', CPU_COHORT_STEPS.first],
    ['Rust / replay measurement (bitget)', 'strict-replay-bitget', CPU_COHORT_STEPS.strict],
    ['Rust / replay measurement (bitget)', 'cpu-runtime-bitget', CPU_COHORT_STEPS.runtime],
    ['Rust / CPU remeasurement (bitget)', 'cpu-second-bitget', CPU_COHORT_STEPS.second],
    ['Rust / CPU remeasurement plan', 'cpu-plan', CPU_COHORT_STEPS.plan],
    ['Rust / replay gate (bitget)', 'cpu-cert-bitget', CPU_COHORT_STEPS.final],
  ]) {
    const value = fixture(), record = value.claims[0], source = record.observations[0];
    source.job.name = jobName; source.job.steps[1].name = stepName; record.artifact.name = artifactName;
    const input = { artifact: record.artifact, artifactBytes: record.artifactBytes, run: value.runs[0], job: source.job, logBytes: log(source.checkout.sha, record.artifact), jobName, stepName, expectedCheckoutSha: source.checkout.sha };
    assert.equal(authenticateArtifactExecution(input).measuredCheckoutSha, source.checkout.sha);
    assert.throws(() => authenticateArtifactExecution({ ...input, stepName: 'arbitrary trusted-looking step' }), /upload step/);
    assert.throws(() => authenticateArtifactExecution({ ...input, jobName: 'Rust / different job' }), /exact attempt1 job/);
    assert.throws(() => authenticateArtifactExecution({ ...input, artifact: { ...input.artifact, name: 'foreign' } }), /exact artifact|authority upload step/);
    assert.throws(() => authenticateArtifactExecution({ ...input, artifact: { ...input.artifact, name: 'strict-replay-binance' } }));
    assert.throws(() => authenticateArtifactExecution({ ...input, job: { ...input.job, run_attempt: 2 } }), /attempt1|physical job/);
    assert.throws(() => authenticateArtifactExecution({ ...input, run: { ...input.run, run_attempt: 2 } }), /attempt1/);
    assert.throws(() => authenticateArtifactExecution({ ...input, expectedCheckoutSha: undefined }), /authenticated expected source/);
    assert.throws(() => authenticateArtifactExecution({ ...input, expectedCheckoutSha: git('unrelated PR merge') }), /authenticated expected source/);
  }
});

test('public measurement clock binds exact unique case markers, source and physical runner', () => {
  const value = fixture(), source = value.claims[0].observations[0], key = value.claims[0].claim.key;
  source.job.steps.splice(1, 0, { name: 'Public data (correctness, CPU and memory)', status: 'completed', conclusion: 'success', started_at: at(5), completed_at: at(18) });
  const execution = { runId: String(value.current.id), jobId: String(source.job.id), runnerId: String(source.job.runner_id), attempt: 1, measuredCheckoutSha: source.checkout.sha, startedAt: at(8), completedAt: at(12) };
  const marker = `${at(8)} CPU_CASE_BEGIN ${key} ${at(8)}\n${at(12)} CPU_CASE_END ${key} ${at(12)}\n`;
  const raw = Buffer.from(Buffer.from(source.logBytes).toString().split('\n').slice(0, 2).join('\n') + '\n' + marker + Buffer.from(source.logBytes).toString().split('\n').slice(2).join('\n'));
  const input = { execution, key, job: source.job, logBytes: raw };
  assert.equal(authenticateMeasurementWindow(input).stepName, 'Public data (correctness, CPU and memory)');
  const mutations = [
    v => { v.execution.runnerId = '999'; }, v => { v.execution.attempt = 2; },
    v => { v.execution.measuredCheckoutSha = git('foreign'); }, v => { v.execution.completedAt = at(8); },
    v => { v.logBytes = Buffer.from(Buffer.from(v.logBytes).toString() + marker); },
    v => { v.logBytes = Buffer.from(Buffer.from(v.logBytes).toString().replace('CPU_CASE_END', 'NO_CASE_END')); },
    v => { v.job.steps[1].name = 'Private correctness'; }, v => { v.job.steps[1].conclusion = 'failure'; },
    v => { v.job.steps.push(clone(v.job.steps[1])); },
    v => { v.logBytes = Buffer.from(Buffer.from(v.logBytes).toString().replace(`${at(8)} CPU_CASE_BEGIN`, `${at(22)} CPU_CASE_BEGIN`)); },
  ];
  for (const mutate of mutations) { const copy = clone(input); mutate(copy); assert.throws(() => authenticateMeasurementWindow(copy)); }
});

test('physical markers remain standalone after child diagnostics without a newline', () => {
  const value = fixture(), source = value.claims[0].observations[0], key = value.claims[0].claim.key;
  source.job.steps.splice(1, 0, { name: 'Public data (correctness, CPU and memory)', status: 'completed', conclusion: 'success', started_at: at(5), completed_at: at(18) });
  const execution = { runId: String(value.current.id), jobId: String(source.job.id), runnerId: String(source.job.runner_id), attempt: 1, measuredCheckoutSha: source.checkout.sha, startedAt: at(8), completedAt: at(12) };
  const timestampLines = (output, time) => output.split('\n').slice(0, -1).map(line => `${time} ${line}\n`).join('');
  const checkout = source.logBytes.toString().split('\n').slice(0, 2).join('\n') + '\n';
  const begin = timestampLines(`prior unterminated output${formatMeasurementMarker('BEGIN', key, at(8))}\n`, at(8));
  const end = timestampLines(`book diagnostic [95100.5,0.00003]${formatMeasurementMarker('END', key, at(12))}\n`, at(12));
  const input = { execution, key, job: source.job, logBytes: Buffer.from(checkout + begin + end) };
  assert.equal(authenticateMeasurementWindow(input).stepName, 'Public data (correctness, CPU and memory)');
  assert.throws(() => authenticateMeasurementWindow({ ...input, logBytes: Buffer.from(checkout + begin + end + end) }), /missing\/duplicate/);
  assert.throws(() => formatMeasurementMarker('OTHER', key, at(8)), /invalid physical/);
  assert.throws(() => formatMeasurementMarker('BEGIN', key, at(8) + '\n'), /invalid execution time|invalid physical marker timestamp/);
  assert.throws(() => formatMeasurementMarker('END', key + '\n', at(12)), /invalid physical/);
});

test('reader caches only immutable Git/ZIP/completed log bytes; mutable authority endpoints refresh', async () => {
  const value = fixture(), api = apiFixture(value), reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: api.fetch }), source = value.claims[0].observations[0];
  for (let index = 0; index < 2; index++) {
    await reader.getRun(value.current.id); await reader.listJobs(value.current.id); await reader.getArtifact(value.claims[0].artifact.id);
    await reader.getCommit(source.checkout.sha); await reader.getTree(source.tree.sha); await reader.getBlob(source.policyBlob.sha);
    await reader.getArtifactBytes(value.claims[0].artifact.id); await reader.getLog(source.job.id);
  }
  const count = suffix => api.calls.filter(call => new URL(call.url).pathname + new URL(call.url).search === `/repos/org/repo/${suffix}`).length;
  assert.equal(count(`actions/runs/${value.current.id}`), 2);
  assert.equal(count(`actions/runs/${value.current.id}/jobs?filter=all&per_page=100&page=1`), 2);
  assert.equal(count(`actions/artifacts/${value.claims[0].artifact.id}`), 2);
  for (const suffix of [`git/commits/${source.checkout.sha}`, `git/trees/${source.tree.sha}?recursive=1`, `git/blobs/${source.policyBlob.sha}`, `actions/artifacts/${value.claims[0].artifact.id}/zip`, `actions/jobs/${source.job.id}/logs`]) assert.equal(count(suffix), 1, suffix);
  const snapshots = reader.snapshots().responses;
  assert.equal(new Set(snapshots.map(row => `${row.path}:${row.sha256}`)).size, snapshots.length);
  assert.deepEqual(reader.snapshots({ since: reader.snapshotCursor() }), { responses: [], logs: [] });
});


test('runtime response admits the authentic 743167768-byte Bybit archive within the existing 4 GiB ZIP bound', () => {
  assert.doesNotThrow(() => requireGithubResponseSize(743_167_768, 'runtime-artifact'));
  assert.doesNotThrow(() => requireGithubResponseSize(4 * 1024 ** 3, 'runtime-artifact'));
  assert.throws(() => requireGithubResponseSize(4 * 1024 ** 3 + 1, 'runtime-artifact'), /oversized/);
});

test('metadata, logs and ordinary archives retain the original 512 MiB bound', () => {
  for (const kind of ['metadata', 'log', 'artifact']) {
    assert.doesNotThrow(() => requireGithubResponseSize(512 * 1024 ** 2, kind));
    assert.throws(() => requireGithubResponseSize(743_167_768, kind), /oversized/);
  }
  for (const length of [0, -1, 0.5, NaN, Infinity])
    assert.throws(() => requireGithubResponseSize(length, 'runtime-artifact'));
  assert.throws(() => requireGithubResponseSize(1, 'unbounded'), /invalid GitHub response kind/);
});

test('immutable artifact recovers one empty HTTP 200 without caching the empty bytes', async () => {
  const calls = [], raw = Buffer.from('complete original archive');
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async (url, options) => {
    calls.push({ url, options }); return new Response(calls.length === 1 ? null : raw);
  } });
  assert.deepEqual(await reader.getArtifactBytes(700), raw);
  assert.deepEqual(await reader.getArtifactBytes(700), raw);
  assert.equal(calls.length, 2); assert.deepEqual(calls[0], calls[1]);
  assert.equal(reader.snapshots().responses.length, 1);
  assert.deepEqual(reader.snapshots().responses[0].raw, raw);
});

test('one empty-artifact recovery and transport errors share the three-attempt cap', async () => {
  for (const failures of [['empty', 'empty'], ['empty', 'socket'], ['socket', 'empty']]) {
    let calls = 0;
    const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => {
      const failure = failures[calls++];
      if (failure === 'socket') throw socketClosure();
      return new Response(null);
    } });
    await assert.rejects(reader.getArtifactBytes(700));
    assert.equal(calls, failures[0] === 'empty' && failures[1] === 'socket' ? 3 : 2, failures.join(' then '));
    assert.equal(reader.snapshots().responses.length, 0);
  }
});

test('empty metadata and completed physical logs are never retried', async () => {
  let metadataCalls = 0, logCalls = 0;
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async url => {
    if (url.endsWith('actions/jobs/5')) return jsonResponse({ id: 5, status: 'completed', conclusion: 'success', completed_at: at(29) });
    if (url.endsWith('/logs')) logCalls++; else metadataCalls++;
    return new Response(null);
  } });
  await assert.rejects(reader.getRun(1000), /empty/); assert.equal(metadataCalls, 1);
  await reader.getJob(5);
  await assert.rejects(reader.getLog(5), /empty/); assert.equal(logCalls, 1);
});

test('empty CDN artifact retry rechecks the original authority and strips its token', async () => {
  const calls = [], raw = Buffer.from('complete original ZIP');
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', token: 'fixture-token', fetch: async (url, options) => {
    calls.push({ url, options });
    if (url.startsWith('https://api.github.com/')) return new Response(null, { status: 302, headers: { location: 'https://fixture.blob.core.windows.net/archive' } });
    return new Response(calls.length === 2 ? null : raw);
  } });
  assert.deepEqual(await reader.getArtifactBytes(700), raw);
  assert.equal(calls.length, 4); assert.deepEqual(calls[0], calls[2]);
  for (const index of [1, 3]) {
    assert.equal(calls[index].options.headers, undefined);
    assert.equal(calls[index].options.redirect, 'error');
  }
});

test('artifact callers cannot request metadata or arbitrary response kinds', async () => {
  let calls = 0;
  const reader = createGithubReader({ sleep: async () => {}, repository: 'org/repo', fetch: async () => { calls++; return new Response('archive'); } });
  for (const kind of ['metadata', 'log', 'unbounded', 4 * 1024 ** 3])
    await assert.rejects(async () => reader.getArtifactBytes(700, kind), /invalid artifact response kind/);
  assert.equal(calls, 0);
});

test('paired baseline PR reads are live, exact and completely paginated', async () => {
  const calls = [], commit = git('paired head');
  const reader = createGithubReader({ repository: 'org/repo', fetch: async url => {
    calls.push(url);
    if (url.includes('/pulls/20')) return jsonResponse({ number: 20, base: { sha: git(String(calls.length)) } });
    return jsonResponse(url.endsWith('page=1') ? Array.from({ length: 100 }, (_, index) => ({ id: index + 1 })) : [{ id: 101 }]);
  } });
  assert.notDeepEqual(await reader.getPull(20), await reader.getPull(20), 'base selection must not cache mutable PR state');
  assert.equal((await reader.listCommitPulls(commit)).length, 101);
  assert.equal(calls.filter(url => url.includes('/commits/')).length, 2);
  await assert.rejects(async () => reader.getPull('../20'));
  await assert.rejects(async () => reader.listCommitPulls('HEAD'));
  const duplicate = createGithubReader({ repository: 'org/repo', fetch: async () => jsonResponse([{ id: 1 }, { id: 1 }]) });
  await assert.rejects(duplicate.listCommitPulls(commit), /duplicate/);
});

test('Rust landing authority reads only the fixed ref and never caches its moving tip', async () => {
  const calls = [];
  const reader = createGithubReader({ repository: 'Tealstreet/tealstreet-sdk', fetch: async url => {
    calls.push(url);
    assert.equal(new URL(url).pathname, '/repos/Tealstreet/tealstreet-sdk/git/ref/heads/feat/rust-rewrite');
    return jsonResponse({ ref: 'refs/heads/feat/rust-rewrite', object: { type: 'commit', sha: git(String(calls.length)) } });
  } });
  assert.notDeepEqual(await reader.getRustLanding(), await reader.getRustLanding());
  assert.equal(calls.length, 2);
});

test('Rust landing ref cannot be read from a head or fork repository', async () => {
  const reader = createGithubReader({ repository: 'fork/repo', fetch: async () => {
    assert.fail('foreign repository must refuse before the network');
  } });
  await assert.rejects(async () => reader.getRustLanding(), /base repository/);
});

test('GraphQL baseRefOid is logged from the fixed base repository and never cached', async () => {
  const calls = [];
  const reader = createGithubReader({ repository: 'Tealstreet/tealstreet-sdk', fetch: async (url, options) => {
    assert.equal(url, 'https://api.github.com/graphql'); assert.equal(options.method, 'POST');
    assert.equal(JSON.parse(options.body).query,
      'query { repository(owner: "Tealstreet", name: "tealstreet-sdk") { pullRequest(number: 2162) { baseRefOid } } }');
    calls.push(options);
    return jsonResponse({ data: { repository: { pullRequest: { baseRefOid: calls.length === 1 ? 'stale' : null } } } });
  } });
  assert.equal(await reader.getPullBaseOid(2162), 'stale');
  assert.equal(await reader.getPullBaseOid(2162), null);
  await assert.rejects(async () => reader.getPullBaseOid('2162) { injected }'), /invalid/);
  assert.equal(calls.length, 2);
  assert.equal(reader.snapshots().responses.length, 2);
});
