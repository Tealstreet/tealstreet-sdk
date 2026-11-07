import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { ISSUE_LABEL, NIGHTLY_ACTOR, failedCpuGates, issueBody, nightlyRuns, sustainedRegressions } from './cpu-nightly.mjs';

const run = (id, extra = {}) => ({ id, event: 'workflow_dispatch', head_branch: 'master', status: 'completed', run_attempt: 1,
  triggering_actor: { login: NIGHTLY_ACTOR }, created_at: `2026-11-${String(id).padStart(2, '0')}T09:17:00Z`,
  head_sha: `${id}`.repeat(40).slice(0, 40), html_url: `https://github.com/x/y/actions/runs/${id}`, ...extra });
const gate = (suite, conclusion) => ({ name: `Rust / CPU gate (${suite})`, conclusion });

test('only completed first-attempt nightly dispatches on master count, newest first', () => {
  const runs = [run(5), run(7), run(6, { triggering_actor: { login: 'sam' } }), run(8, { status: 'in_progress' }),
    run(9, { head_branch: 'feature' }), run(10, { event: 'push' }), run(11, { run_attempt: 2 })];
  assert.deepEqual(nightlyRuns(runs).map((value) => value.id), [7, 5]);
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
