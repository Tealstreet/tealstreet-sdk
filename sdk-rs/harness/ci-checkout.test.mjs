import assert from 'node:assert/strict';
import test from 'node:test';
import { selectDispatchedCheckout } from './ci-checkout.mjs';
const A = 'a'.repeat(40), H = 'b'.repeat(40), M = 'c'.repeat(40), T = 'd'.repeat(40), L = 'e'.repeat(40);
const repo = 'Tealstreet/tealstreet-sdk';
function fixture() {
  const run = { id: 10, run_attempt: 1, event: 'workflow_dispatch', head_sha: H, head_branch: 'feature', repository: { full_name: repo } };
  const pull = { number: 12, state: 'open', head: { sha: H, repo: { full_name: repo } },
    base: { sha: 'f'.repeat(40), ref: 'feat/rust-rewrite', repo: { full_name: repo } } };
  const commit = { sha: M, tree: { sha: T }, parents: [{ sha: A }, { sha: H }] };
  const state = { checkout: H, parents: [A, H], tree: T, firstParents: [L, A], fetched: L, mergeTree: T, origin: `https://github.com/${repo}.git` };
  const calls = [], reader = { getRun: async () => run, listCommitPulls: async () => [pull], getPull: async () => pull,
    getCommit: async () => commit, getRustLanding: async () => ({ ref: 'refs/heads/feat/rust-rewrite', object: { type: 'commit', sha: L },
      url: `https://api.github.com/repos/${repo}/git/refs/heads/feat/rust-rewrite` }) };
  const git = (...args) => {
    calls.push(args);
    if (args[0] === 'fetch') return '';
    if (args[0] === 'remote') return state.origin;
    if (args[0] === 'checkout') { state.checkout = args[2]; return ''; }
    if (args[0] === 'show') return state.parents.join(' ');
    if (args[0] === 'rev-list') return state.firstParents.join('\n');
    if (args[0] === 'merge-tree') return state.mergeTree;
    if (args[0] === 'rev-parse') return args[1] === 'HEAD' ? state.checkout : args[1] === 'FETCH_HEAD' ? M
      : args[1] === 'refs/remotes/origin/feat/rust-rewrite' ? state.fetched : state.tree;
    throw new Error(`Unexpected git command ${args}`);
  };
  return { run, pull, commit, state, calls, context: { reader, runId: 10, git } };
}

test('a fresh dispatch selects the authenticated PR merge, not its stale metadata base or feature head', async () => {
  const x = fixture(); assert.equal(await selectDispatchedCheckout(x.context), M);
  assert.equal(x.state.checkout, M);
  assert(x.calls.some(args => args[0] === 'fetch' && args[3] === 'refs/pull/12/merge'));
  assert(x.calls.some(args => args[0] === 'merge-tree' && args[2] === A && args[3] === H));
});

test('dispatch refuses source, run, parent, tree, fork and landing substitutions', async t => {
  for (const [name, mutate] of [
    ['retried run', x => { x.run.run_attempt = 2; }],
    ['wrong initial checkout', x => { x.state.checkout = A; }],
    ['wrong event', x => { x.run.event = 'pull_request'; }],
    ['changed head', x => { x.pull.head.sha = A; }],
    ['fork', x => { x.pull.head.repo.full_name = 'fork/sdk'; }],
    ['base ref', x => { x.pull.base.ref = 'master'; }],
    ['origin', x => { x.state.origin = 'https://github.com/fork/sdk.git'; }],
    ['wrong parent2', x => { x.state.parents = [A, L]; x.commit.parents[1].sha = L; }],
    ['one parent', x => { x.state.parents = [A]; x.commit.parents.pop(); }],
    ['extra parent', x => { x.state.parents.push(L); x.commit.parents.push({ sha: L }); }],
    ['API/local disagreement', x => { x.commit.parents[0].sha = L; }],
    ['API tree', x => { x.commit.tree.sha = A; }],
    ['merge tree', x => { x.state.mergeTree = A; }],
    ['side-merge-only ancestor', x => { x.state.firstParents = [L]; }],
    ['ref API/fetch race', x => { x.state.fetched = A; }],
  ]) await t.test(name, async () => { const x = fixture(); mutate(x); await assert.rejects(selectDispatchedCheckout(x.context)); });
});

test('a dispatch on the actual landing branch retains its exact API run head', async () => {
  const x = fixture(); x.run.head_branch = 'feat/rust-rewrite';
  x.context.reader.listCommitPulls = () => { throw new Error('Landing is not a PR head'); };
  assert.equal(await selectDispatchedCheckout(x.context), H);
  assert.deepEqual(x.calls, [['rev-parse', 'HEAD']]);
});
