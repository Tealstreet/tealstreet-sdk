import assert from 'node:assert/strict';
import test from 'node:test';
import { selectBitgetPair } from './bitget-paired-ci.mjs';

const A = 'a'.repeat(40), B = 'b'.repeat(40), H = 'c'.repeat(40), TREE = 'd'.repeat(40);
function fixture() {
  const run = { id: 10, run_attempt: 1, head_sha: H, event: 'pull_request',
    repository: { full_name: 'Tealstreet/tealstreet-sdk' } };
  const pull = { number: 20, id: 21, state: 'open', head: { sha: H },
    base: { sha: A, ref: 'feat/rust-rewrite', repo: { full_name: 'Tealstreet/tealstreet-sdk' } } };
  run.pull_requests = [structuredClone(pull)];
  const jobs = [{ id: 30, runner_id: 40, run_id: 10, run_attempt: 1, head_sha: H,
    name: 'Rust / Bitget counterbalanced steady CPU' }];
  const pulls = [pull], calls = [];
  const reader = { getRun: async () => run, listJobs: async () => jobs,
    listCommitPulls: async sha => { assert.equal(sha, H); return pulls; },
    getPull: async number => { assert.equal(number, 20); return pull; },
    getCommit: async sha => { assert.equal(sha, H); return { sha: H, tree: { sha: TREE }, parents: [{ sha: A }] }; } };
  const gitRead = (...args) => {
    calls.push(args);
    if (args[0] === 'merge-tree') { assert.deepEqual(args, ['merge-tree', '--write-tree', A, H]); return TREE; }
    return args[1] === 'HEAD' ? B : TREE;
  };
  return { run, pull, pulls, jobs, calls, context: { reader, runId: 10, root: '/fixture', gitRead } };
}

test('baseline comes from exact authenticated PR API and full composition is recomputed', async () => {
  const x = fixture(), result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, B); assert.equal(result.head, H);
  assert.ok(x.calls.some(args => args[0] === 'merge-tree'));
});

test('ambiguous, stale, cross-branch or retried source/producer fails closed', async () => {
  for (const mutate of [x => x.pulls.push(x.pull), x => x.jobs.push(x.jobs[0]),
    x => { x.pulls.length = 0; }, x => { x.pull.state = 'closed'; }, x => { x.pull.base.ref = 'master'; },
    x => { x.pull.head.sha = A; }, x => { x.run.run_attempt = 2; }, x => { x.jobs[0].run_attempt = 2; },
    x => { x.run.pull_requests = []; },
    x => { x.context.gitRead = (...args) => args[0] === 'merge-tree' ? A : args[1] === 'HEAD' ? B : TREE; }]) {
    const x = fixture(); mutate(x); await assert.rejects(selectBitgetPair(x.context));
  }
});

test('a later target landing cannot silently move the original run baseline', async () => {
  const x = fixture(); x.pull.base.sha = 'e'.repeat(40);
  assert.equal((await selectBitgetPair(x.context)).baseline, A);
});

test('landing push uses the authenticated actual first parent and exact landing tree', async () => {
  const x = fixture(); x.run.event = 'push'; x.run.head_branch = 'feat/rust-rewrite';
  x.context.gitRead = (...args) => args[1] === 'HEAD' ? H : TREE;
  x.context.reader.listCommitPulls = async () => { throw new Error('push must not select a PR baseline'); };
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, H);
});

test('master pushes cannot obtain a Rust paired certificate', async () => {
  const x = fixture(); x.run.event = 'push'; x.run.head_branch = 'master';
  await assert.rejects(selectBitgetPair(x.context), /Rust landing branch/);
});
