import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { ISSUE_LABEL, NIGHTLY_ACTOR, consecutiveNights, evaluate, failedCpuGates, issueBody, nightlyRuns, sustainedRegressions } from './cpu-nightly.mjs';

const run = (id, extra = {}) => ({ id, event: 'workflow_dispatch', head_branch: 'master', status: 'completed', run_attempt: 1,
  triggering_actor: { login: NIGHTLY_ACTOR }, created_at: `2026-11-${String(id).padStart(2, '0')}T09:17:00Z`,
  head_sha: `${id}`.repeat(40).slice(0, 40), html_url: `https://github.com/x/y/actions/runs/${id}`, ...extra });
const gate = (suite, conclusion) => ({ name: `Rust / CPU gate (${suite})`, conclusion });

test('only completed first-attempt nightly dispatches on master count, newest first', () => {
  const runs = [run(5), run(7), run(6, { triggering_actor: { login: 'sam' } }), run(8, { status: 'in_progress' }),
    run(9, { head_branch: 'feature' }), run(10, { event: 'push' }), run(11, { run_attempt: 2 })];
  assert.deepEqual(nightlyRuns(runs).map((value) => value.id), [7, 5]);
});

test('one result per UTC night: the last run that night; adjacency is by calendar night', () => {
  const runs = [run(1, { created_at: '2026-11-07T09:17:00Z' }), run(2, { created_at: '2026-11-07T15:00:00Z' }),
    run(3, { created_at: '2026-11-06T09:17:00Z' })];
  assert.deepEqual(nightlyRuns(runs).map((value) => value.id), [2, 3]);
  const at = (created_at) => ({ created_at });
  assert.equal(consecutiveNights(at('2026-11-07T00:05:00Z'), at('2026-11-06T23:55:00Z')), true);
  assert.equal(consecutiveNights(at('2026-11-07T09:17:00Z'), at('2026-11-04T09:17:00Z')), false);
  assert.equal(consecutiveNights(at('2026-11-07T09:17:00Z'), at('2026-11-07T08:00:00Z')), false);
  assert.equal(consecutiveNights(at('2026-11-07T09:17:00Z'), undefined), false);
});

test('only failed CPU gates name a suite', () => {
  assert.deepEqual(failedCpuGates([gate('bybit', 'failure'), gate('okx', 'success'), gate('bitget', 'failure'),
    { name: 'Rust / replay measurement (woo)', conclusion: 'failure' }, gate('bybit', 'failure')]), ['bitget', 'bybit']);
});

test('a regression is sustained only when the same suite fails on both of the two latest nights', () => {
  assert.deepEqual(sustainedRegressions(['bitget', 'bybit'], ['bybit', 'okx']), ['bybit']);
  assert.deepEqual(sustainedRegressions(['bybit'], []), [], 'one night is not sustained');
  assert.deepEqual(sustainedRegressions(['bybit'], undefined), [], 'fewer than two nights is never sustained');
  assert.deepEqual(sustainedRegressions([], ['bybit']), [], 'cleared tonight');
});

test('the issue names the suites, both runs and the policy', () => {
  const body = issueBody({ suites: ['bybit'], runs: [run(7), run(5)] });
  assert.match(body, /> 1\.00x after the one in-job re-measure/);
  assert.match(body, /- `bybit`/);
  assert.match(body, /actions\/runs\/7/);
  assert.match(body, /actions\/runs\/5/);
  assert.match(body, /Sam, 2026-11-07/);
});

test('the nightly workflow evaluates the previous two nights, then dispatches tonight on master', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust-cpu-nightly.yml', import.meta.url), 'utf8');
  assert.match(workflow, /schedule:\n\s+- cron: '/);
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /issues: write/);
  assert.match(workflow, /actions: write/);
  const evaluate = workflow.indexOf('cpu-nightly.mjs --command evaluate'), dispatch = workflow.indexOf('gh workflow run rust.yml --ref master');
  assert.ok(evaluate > 0 && dispatch > evaluate, 'evaluate completed nights before dispatching a new one');
  assert.equal(ISSUE_LABEL, 'cpu-regression');
});

// #3248 review: the real evaluate() over a mocked GitHub API.
async function issueWrites(dates) {
  const saved = globalThis.fetch, writes = [];
  globalThis.fetch = async (url, init = {}) => {
    if (url.includes('/actions/workflows/')) return Response.json({ workflow_runs: dates.map((created_at, index) => run(index + 1, { created_at })) });
    if (url.includes('/jobs?')) return Response.json({ jobs: [gate('bybit', 'failure')] });
    if (url.includes('/issues?')) return Response.json([]);
    if (url.endsWith('/issues') && init.method === 'POST') { writes.push(JSON.parse(init.body)); return Response.json({ number: 1 }); }
    throw new Error(`Unexpected request ${url}`);
  };
  try { await evaluate({ token: 'fixture-token', repository: 'fixture/repo', log: () => {} }); return writes; }
  finally { globalThis.fetch = saved; }
}
test('adjacent nights both failing open one cpu-regression issue', async () => {
  const writes = await issueWrites(['2026-11-06T09:17:00Z', '2026-11-07T09:17:00Z']);
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].labels, [ISSUE_LABEL]);
});
test('two dispatches on the same night cannot file a sustained regression', async () => {
  assert.equal((await issueWrites(['2026-11-07T09:20:00Z', '2026-11-07T10:20:00Z'])).length, 0);
});
test('nonconsecutive nights cannot file a sustained regression', async () => {
  assert.equal((await issueWrites(['2026-11-04T09:17:00Z', '2026-11-07T09:17:00Z'])).length, 0);
});

test('nightly evaluation paginates until two distinct nightly dates or exhausted history', async (t) => {
  const manual = Array.from({ length: 49 }, (_, index) => run(100 + index, {
    triggering_actor: { login: 'sam' }, created_at: '2026-11-07T10:00:00Z',
  }));
  const sameNight = Array.from({ length: 50 }, (_, index) => run(200 + index, { created_at: '2026-11-07T09:17:00Z' }));
  for (const { name, pages, expectedWrites } of [
    { name: 'manual runs push the previous nightly onto page two', pages: [[...manual, run(7)], [run(6)]], expectedWrites: 1 },
    { name: 'multiple runs on one night do not stop pagination', pages: [sameNight, [run(6)]], expectedWrites: 1 },
    { name: 'history ends without a second eligible night', pages: [[...manual, run(7)], []], expectedWrites: 0 },
  ]) await t.test(name, async () => {
    const saved = globalThis.fetch, requestedPages = [], writes = [], jobRuns = [];
    globalThis.fetch = async (url, init = {}) => {
      const parsed = new URL(url);
      if (parsed.pathname.includes('/actions/workflows/')) {
        const page = Number(parsed.searchParams.get('page') ?? 1);
        requestedPages.push(page);
        assert.equal(parsed.searchParams.get('per_page'), '50');
        assert.ok(page <= pages.length, 'stop once two nightly dates are found or history is exhausted');
        return Response.json({ workflow_runs: pages[page - 1] });
      }
      if (url.includes('/jobs?')) { jobRuns.push(url); return Response.json({ jobs: [gate('bybit', 'failure')] }); }
      if (url.includes('/issues?')) return Response.json([]);
      if (url.endsWith('/issues') && init.method === 'POST') { writes.push(JSON.parse(init.body)); return Response.json({ number: 1 }); }
      throw new Error(`Unexpected request ${url}`);
    };
    try {
      await evaluate({ token: 'fixture-token', repository: 'fixture/repo', log: () => {} });
      assert.deepEqual(requestedPages, [1, 2]);
      assert.equal(writes.length, expectedWrites);
      assert.equal(jobRuns.length, expectedWrites ? 2 : 0);
      if (expectedWrites) assert.match(writes[0].body, /actions\/runs\/6/);
    } finally { globalThis.fetch = saved; }
  });
});
