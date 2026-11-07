import assert from 'node:assert/strict';
import test from 'node:test';
import { createGithubReader } from './cpu-cohort.mjs';
import { evidenceEndCheck } from './cpu-ci.mjs';

const epipe = () => new TypeError('fetch failed', { cause: { code: 'EPIPE' } });
function endCheck({ mismatch = false, failure = epipe } = {}) {
  const run = { id: 1000, run_attempt: 1 };
  const job = { id: 10, name: 'Rust / replay measurement (bitget)', runner_id: 20 };
  const artifact = { id: 30, name: 'strict-replay-bitget', digest: `sha256:${'a'.repeat(64)}` };
  const calls = [], delays = [];
  let jobCalls = 0;
  const reader = createGithubReader({ repository: 'org/repo', token: 'fixture-token',
    sleep: async ms => { delays.push(ms); }, fetch: async (url, options) => {
      calls.push({ url, options });
      if (url.includes('/jobs?')) {
        if (++jobCalls === 1) throw failure();
        return Response.json({ total_count: 1, jobs: [{ ...job, ...(mismatch ? { runner_id: 99 } : {}) }] });
      }
      if (url.includes('/artifacts?')) return Response.json({ total_count: 1, artifacts: [artifact] });
      return Response.json(run);
    } });
  return { context: { runId: run.id, reader, bindings: [{ job, artifact }] }, calls, delays, jobCalls: () => jobCalls };
}

test('original RED: end-check recovers EPIPE without repeating timing or changing bound evidence', async () => {
  const value = endCheck();
  await evidenceEndCheck(value.context);
  assert.equal(value.jobCalls(), 2);
  assert.deepEqual(value.delays, [1000]);
  const attempts = value.calls.filter(call => call.url.includes('/jobs?'));
  assert.deepEqual(attempts[0], attempts[1]);
  assert.equal(value.context.reader.snapshots().responses.length, 3);
});

test('original RED: a real producer mismatch still fails after EPIPE recovery without retrying the verdict', async () => {
  const value = endCheck({ mismatch: true });
  await assert.rejects(evidenceEndCheck(value.context), /physical producer became duplicate\/changed/);
  assert.equal(value.jobCalls(), 2);
  assert.deepEqual(value.delays, [1000]);
});

for (const status of [500, 502, 503, 504, 599]) test(`original RED: HTTP${status} retrieval recovers under the same fixed backoff`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms),
    fetch: async () => ++calls === 1 ? new Response('temporary', { status }) : Response.json({ id: 1000 }) });
  assert.deepEqual(await reader.getRun(1000), { id: 1000 });
  assert.equal(calls, 2); assert.deepEqual(delays, [1000]);
});

for (const failBody of [false, true]) test(`original RED: metadata EPIPE ${failBody ? 'body' : 'fetch'} retry retains only complete bytes`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => {
    if (++calls === 1) {
      if (failBody) return { status: 200, arrayBuffer: async () => { throw epipe(); } };
      throw epipe();
    }
    return Response.json({ id: 1000 });
  } });
  assert.deepEqual(await reader.getRun(1000), { id: 1000 }); assert.equal(calls, 2);
  assert.deepEqual(delays, [1000]);
  assert.equal(reader.snapshots().responses.length, 1);
  assert.equal(reader.snapshots().responses[0].raw.toString(), '{"id":1000}');
});

for (const shape of ['network', 'server']) test(`original RED: permanent ${shape} failure stops after three attempts and two backoffs`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => {
    calls++; if (shape === 'network') throw epipe(); return new Response('temporary', { status: 502 });
  } });
  await assert.rejects(reader.getRun(1000), shape === 'network' ? /fetch failed/ : /GitHub evidence unavailable \(502\)/);
  assert.equal(calls, 3); assert.deepEqual(delays, [1000, 2000]);
  assert.equal(reader.snapshots().responses.length, 0);
});

for (const status of [400, 401, 403, 404, 429]) test(`working control: HTTP${status} is not retried`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => {
    calls++; return new Response('refused', { status });
  } });
  await assert.rejects(reader.getRun(1000), new RegExp(`GitHub evidence unavailable \\(${status}\\)`));
  assert.equal(calls, 1); assert.deepEqual(delays, []);
});

for (const raw of ['{"id":1,"id":2}', '{', '']) test(`working control: malformed or empty metadata ${JSON.stringify(raw)} never retries`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => { calls++; return new Response(raw); } });
  await assert.rejects(reader.getRun(1000)); assert.equal(calls, 1); assert.deepEqual(delays, []);
});

test('original RED: redirect transport retry repeats the API authority and never forwards its token to CDN', async () => {
  const calls = [], delays = []; let downloads = 0;
  const reader = createGithubReader({ repository: 'org/repo', token: 'fixture-token', sleep: async ms => delays.push(ms), fetch: async (url, options) => {
    calls.push({ url, options });
    if (url.startsWith('https://api.github.com/')) return new Response(null, { status: 302, headers: { location: 'https://fixture.blob.core.windows.net/archive' } });
    if (++downloads === 1) throw epipe();
    return new Response('complete ZIP fixture');
  } });
  assert.equal((await reader.getArtifactBytes(700)).toString(), 'complete ZIP fixture');
  assert.deepEqual(calls[0], calls[2]);
  for (const index of [1, 3]) { assert.equal(calls[index].options.headers, undefined); assert.equal(calls[index].options.redirect, 'error'); }
  assert.deepEqual(delays, [1000]); assert.equal(reader.snapshots().responses.length, 1);
});

test('working control: untrusted redirect is a fatal verdict, never a transport retry', async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => {
    calls++; return new Response(null, { status: 302, headers: { location: 'https://evil.example/archive' } });
  } });
  await assert.rejects(reader.getArtifactBytes(700), /untrusted/); assert.equal(calls, 1); assert.deepEqual(delays, []);
});

for (const [sequence, expectedCalls, succeeds] of [
  [['empty', 'network', 'server'], 3, false],
  [['server', 'empty'], 2, false],
  [['network', 'server', 'success'], 3, true],
  [['empty', 'server', 'success'], 3, true],
  [['server', 'network', 'server'], 3, false],
]) test(`mixed retrieval budget: ${sequence.join(' then ')}`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => {
    const next = sequence[calls++]; assert.notEqual(next, undefined, 'no retrieval outside the shared cap');
    if (next === 'network') throw epipe();
    return new Response(next === 'empty' ? null : next === 'server' ? 'temporary' : 'complete archive', { status: next === 'server' ? 502 : 200 });
  } });
  if (succeeds) {
    assert.equal((await reader.getArtifactBytes(700)).toString(), 'complete archive');
    assert.equal((await reader.getArtifactBytes(700)).toString(), 'complete archive', 'only complete original bytes are cached');
  } else await assert.rejects(reader.getArtifactBytes(700));
  assert.equal(calls, expectedCalls); assert.deepEqual(delays, expectedCalls === 3 ? [1000, 2000] : [1000]);
  assert.equal(reader.snapshots().responses.length, succeeds ? 1 : 0);
});

for (const failure of [new TypeError('unknown body failure', { cause: { code: 'OTHER' } }),
  new Error('provenance mismatch', { cause: { code: 'EPIPE' } })]) test(`working body control: ${failure.message} is never transport recovery`, async () => {
  let calls = 0; const delays = [];
  const reader = createGithubReader({ repository: 'org/repo', sleep: async ms => delays.push(ms), fetch: async () => {
    calls++; return { status: 200, arrayBuffer: async () => { throw failure; } };
  } });
  await assert.rejects(reader.getRun(1000), error => error === failure);
  assert.equal(calls, 1); assert.deepEqual(delays, []); assert.equal(reader.snapshots().responses.length, 0);
});

test('CDN HTTP5xx exhaustion shares three authority attempts and strips authentication each time', async () => {
  const calls = [], delays = [];
  const reader = createGithubReader({ repository: 'org/repo', token: 'fixture-token', sleep: async ms => delays.push(ms), fetch: async (url, options) => {
    calls.push({ url, options });
    if (url.startsWith('https://api.github.com/')) return new Response(null, { status: 302, headers: { location: 'https://fixture.blob.core.windows.net/archive' } });
    return new Response('temporary', { status: 503 });
  } });
  await assert.rejects(reader.getArtifactBytes(700), /GitHub evidence unavailable \(503\)/);
  assert.equal(calls.length, 6); assert.deepEqual(delays, [1000, 2000]);
  for (const i of [0, 2, 4]) assert.deepEqual(calls[i], calls[0]);
  for (const i of [1, 3, 5]) assert.equal(calls[i].options.headers, undefined);
  assert.equal(reader.snapshots().responses.length, 0);
});
