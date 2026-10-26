import assert from 'node:assert/strict';
import test, { mock } from 'node:test';
import { BITGET_CUMULATIVE_ANCHOR } from './bitget-paired-steady.mjs';
import { selectBitgetPair } from './bitget-paired-ci.mjs';

const A = 'a'.repeat(40), B = 'b'.repeat(40), H = 'c'.repeat(40), TREE = 'd'.repeat(40);
function fixture() {
  const run = { id: 10, run_attempt: 1, head_sha: H, event: 'pull_request',
    repository: { full_name: 'Tealstreet/tealstreet-sdk' } };
  const pull = { number: 20, id: 21, state: 'open', head: { sha: H, repo: { url: 'https://api.github.com/repos/Tealstreet/tealstreet-sdk' } },
    base: { sha: A, ref: 'feat/rust-rewrite', repo: { full_name: 'Tealstreet/tealstreet-sdk', url: 'https://api.github.com/repos/Tealstreet/tealstreet-sdk' } } };
  run.pull_requests = [structuredClone(pull)];
  const jobs = [{ id: 30, runner_id: 40, run_id: 10, run_attempt: 1, head_sha: H,
    name: 'Rust / Bitget counterbalanced steady CPU' }];
  const pulls = [pull], calls = [];
  const state = { checkout: { sha: B, tree: { sha: TREE }, parents: [{ sha: A }, { sha: H }] },
    localParents: [A, H], firstParents: [A, BITGET_CUMULATIVE_ANCHOR], ancestor: true, landingTip: A, localLanding: A, mergeTree: TREE, origin: 'https://github.com/Tealstreet/tealstreet-sdk' };
  const reader = { getRun: async () => run, listJobs: async () => jobs,
    getRustLanding: async () => ({ ref: 'refs/heads/feat/rust-rewrite',
      url: 'https://api.github.com/repos/Tealstreet/tealstreet-sdk/git/refs/heads/feat/rust-rewrite', object: { type: 'commit', sha: state.landingTip } }),
    getPullBaseOid: async number => { assert.equal(number, 20); return pull.base.sha; },
    listCommitPulls: async sha => { assert.equal(sha, H); return pulls; },
    getPull: async number => { assert.equal(number, 20); return pull; },
    getCommit: async sha => {
      if (sha === B) return state.checkout;
      assert.equal(sha, H); return { sha: H, tree: { sha: TREE }, parents: [{ sha: A }] };
    } };
  const gitRead = (...args) => {
    calls.push(args);
    if (args[0] === 'show') { assert.deepEqual(args, ['show', '-s', '--format=%P', B]); return state.localParents.join(' '); }
    if (args[0] === 'remote') { assert.deepEqual(args, ['remote', 'get-url', 'origin']); return state.origin; }
    if (args[0] === 'fetch') { assert.deepEqual(args, ['fetch', '--no-tags', 'origin',
      'refs/heads/feat/rust-rewrite:refs/remotes/origin/feat/rust-rewrite']); return ''; }
    if (args[0] === 'rev-parse' && args[1] === 'refs/remotes/origin/feat/rust-rewrite') return state.localLanding;
    if (args[0] === 'merge-base') {
      assert.deepEqual(args.slice(0,3), ['merge-base', '--is-ancestor', state.checkout.parents[0].sha]);
      assert.equal(args[3], state.landingTip);
      if (!state.ancestor) throw new Error('not an ancestor');
      return '';
    }
    if (args[0] === 'rev-list') { assert.deepEqual(args.slice(0,2), ['rev-list', '--first-parent']);
      assert.equal(args[2], state.landingTip); return state.firstParents.join('\n'); }
    if (args[0] === 'merge-tree') { assert.deepEqual(args, ['merge-tree', '--write-tree', A, H]); return state.mergeTree; }
    return args[1] === 'HEAD' ? B : TREE;
  };
  return { run, pull, pulls, jobs, calls, state, context: { reader, runId: 10, root: '/fixture', gitRead } };
}

test('baseline comes from the authenticated checkout first parent on the actual landing chain', async () => {
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
  const x = fixture(); x.state.landingTip = 'e'.repeat(40); x.state.localLanding = x.state.landingTip;
  x.state.firstParents.unshift(x.state.landingTip);
  x.pull.base.sha = 'stale metadata is informational';
  x.run.pull_requests[0].base.sha = null;
  assert.equal((await selectBitgetPair(x.context)).baseline, A);
});

test('landing push uses the authenticated actual first parent and exact landing tree', async () => {
  const x = fixture(); x.run.event = 'push'; x.run.head_branch = 'feat/rust-rewrite';
  x.context.gitRead = (...args) => args[1] === 'HEAD' ? H : TREE;
  x.context.reader.listCommitPulls = async () => { throw new Error('push must not select a PR baseline'); };
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, H);
});

test('a fresh dispatch on the landing branch uses that landing, without requiring an open PR', async () => {
  const x = fixture(); x.run.event = 'workflow_dispatch'; x.run.head_branch = 'feat/rust-rewrite';
  x.context.gitRead = (...args) => args[1] === 'HEAD' ? H : TREE;
  x.context.reader.listCommitPulls = async () => { throw new Error('landing dispatch is not a PR head'); };
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, H);
});

test('master pushes cannot obtain a Rust paired certificate', async () => {
  const x = fixture(); x.run.event = 'push'; x.run.head_branch = 'master';
  await assert.rejects(selectBitgetPair(x.context), /Rust landing branch/);
});

test('checkout API/local parents and trees, exact head and first-parent landing ancestry fail closed', async t => {
  const cases = [
    ['exact automatic tree differs from checkout', x => { x.state.mergeTree = A; }, /complete automatic composition/],
    ['current-ref API error', x => { x.context.reader.getRustLanding = async () => { throw new Error('ref API unavailable'); }; }, /ref API unavailable/],
    ['local origin points at a fork', x => { x.state.origin = 'https://github.com/fork/tealstreet-sdk'; }, /authenticated base repository/],
    ['wrong parent two', x => { x.state.checkout.parents[1].sha = A; x.state.localParents[1] = A; }, /checkout API\/local/],
    ['wrong tree', x => { x.state.checkout.tree.sha = A; }, /checkout API\/local/],
    ['ancestor reachable only through a merged branch', x => { x.state.firstParents = [H]; }, /actual first-parent/],
    ['fixed anchor is not on the live first-parent chain', x => { x.state.firstParents = [A]; }, /fixed cumulative anchor/],
    ['not a live-target ancestor', x => { x.state.ancestor = false; }, /not an ancestor/],
    ['single-parent checkout', x => { x.state.checkout.parents.pop(); x.state.localParents.pop(); }, /checkout API\/local/],
    ['API and local parents disagree', x => { x.state.localParents[0] = H; }, /checkout API\/local/],
    ['extra checkout parent', x => { x.state.checkout.parents.push({ sha: A }); x.state.localParents.push(A); }, /checkout API\/local/],
    ['wrong API commit identity', x => { x.state.checkout.sha = H; }, /checkout API\/local/],
    ['foreign current landing ref', x => {
      x.context.reader.getRustLanding = async () => ({ ref: 'refs/heads/master', object: { type: 'commit', sha: A } });
    }, /current Rust landing ref/],
    ['landing ref from a fork repository', x => {
      const old = x.context.reader.getRustLanding;
      x.context.reader.getRustLanding = async () => ({ ...await old(),
        url: 'https://api.github.com/repos/fork/tealstreet-sdk/git/refs/heads/feat/rust-rewrite' });
    }, /ref from the base repository/],
    ['API and local landing ref disagree', x => { x.state.localLanding = H; }, /API and local current Rust landing ref/],
    ['pull base repository is foreign', x => { x.pull.base.repo.full_name = 'fork/tealstreet-sdk'; }, /exact open PR/],
    ['not a current landing ancestor', x => {
      x.state.landingTip = 'e'.repeat(40); x.state.localLanding = x.state.landingTip;
      const old = x.context.gitRead;
      x.context.gitRead = (...args) => {
        if (args[0] === 'merge-base' && args[3] === x.state.landingTip) throw new Error('not a current landing ancestor');
        return old(...args);
      };
    }, /not a current landing ancestor/],
  ];
  for (const [name, mutate, error] of cases) await t.test(name, async () => {
    const x = fixture(); mutate(x);
    await assert.rejects(selectBitgetPair(x.context), error);
  });
});

test('real run 40238038972 uses checkout base 035948 even though both metadata bases still say f895', async () => {
  const x = fixture();
  const stale = 'f895d0d70187ee37d2af4bbe17ee81d51cc5c81e';
  const landed = '035948b564a9338371be2b2aef4cad073e7302a9';
  const head = '1bc7c877fcb6d256a0fe529bd858dd1bfd475bb7';
  const checkout = '8e6e70158a63e865dfa1476ca9bcd8a60fe68215';
  const actualTree = '7bbd29d4b23c6e558646c2c71e47fc6498693233';
  const staleTree = 'de9bc741a2951f75f588891aed7b9d04d09eff09';
  x.run.head_sha = head; x.pull.head.sha = head; x.pull.base.sha = stale;
  x.run.pull_requests = [structuredClone(x.pull)]; x.run.pull_requests[0].base.sha = stale;
  x.jobs[0].head_sha = head;
  x.context.reader.listCommitPulls = async sha => { assert.equal(sha, head); return [x.pull]; };
  x.state.landingTip = '49a5b54f36232d1c926f4280141d92060db18e2b';
  x.context.reader.getCommit = async sha => { assert.equal(sha, checkout); return { sha: checkout,
    tree: { sha: actualTree }, parents: [{ sha: landed }, { sha: head }] }; };
  x.context.gitRead = (...args) => {
    x.calls.push(args);
    if (args[0] === 'rev-parse') return args[1] === 'HEAD' ? checkout :
      args[1] === 'refs/remotes/origin/feat/rust-rewrite' ? x.state.landingTip : actualTree;
    if (args[0] === 'remote') return 'https://github.com/Tealstreet/tealstreet-sdk.git';
    if (args[0] === 'show') return `${landed} ${head}`;
    if (args[0] === 'fetch') { assert.equal(args[3], 'refs/heads/feat/rust-rewrite:refs/remotes/origin/feat/rust-rewrite'); return ''; }
    if (args[0] === 'merge-base') { assert.equal(args[3], x.state.landingTip); return ''; }
    if (args[0] === 'rev-list') { assert.equal(args[2], x.state.landingTip); return `${x.state.landingTip}\n${landed}\n${stale}\n${BITGET_CUMULATIVE_ANCHOR}`; }
    assert.deepEqual(args.slice(0, 2), ['merge-tree', '--write-tree']);
    assert.equal(args[3], head);
    return args[2] === landed ? actualTree : staleTree;
  };
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, landed);
  assert.equal(result.candidate, checkout);
  assert.equal(result.candidateTree, actualTree);
});

test('informational GraphQL errors and differing OIDs cannot change real checkout selection', async t => {
  for (const throws of [true, false]) await t.test(throws ? 'GraphQL throws' : 'GraphQL OID differs', async () => {
    const x = fixture();
    const landed = '035948b564a9338371be2b2aef4cad073e7302a9';
    x.state.checkout.parents[0].sha = landed;
    x.state.localParents[0] = landed;
    x.state.firstParents.unshift(landed);
    x.context.reader.getPullBaseOid = async () => {
      if (throws) throw new Error('GraphQL HTTP 503');
      return H;
    };
    const original = x.context.gitRead;
    x.context.gitRead = (...args) => args[0] === 'merge-tree' ? TREE : original(...args);
    const lines = [];
    const logger = mock.method(console, 'log', line => lines.push(JSON.parse(line)));
    try {
      const selected = await selectBitgetPair(x.context);
      assert.equal(selected.baseline, landed);
      const logged = lines[0].bitgetBaseSelection;
      assert.equal(logged.graphqlBaseRefOid, throws ? null : H);
      assert.equal(logged.graphqlError, throws ? 'GraphQL HTTP 503' : null);
      assert.equal(logged.baseline, landed);
    } finally {
      logger.mock.restore();
    }
  });
});
