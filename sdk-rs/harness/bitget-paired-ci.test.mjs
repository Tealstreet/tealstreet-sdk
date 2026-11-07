import assert from 'node:assert/strict';
import test, { mock } from 'node:test';
import { BITGET_CUMULATIVE_ANCHOR } from './bitget-paired-steady.mjs';
import { familyComparisonAuthority } from './cpu-family-identity.mjs';
import { RETIRED_RUST_LANDING_TIP, selectBitgetPair } from './bitget-paired-ci.mjs';

const A = 'a'.repeat(40), B = 'b'.repeat(40), H = 'c'.repeat(40), TREE = 'd'.repeat(40);
function fixture() {
  const run = { id: 10, run_attempt: 1, head_sha: H, event: 'pull_request',
    repository: { full_name: 'Tealstreet/tealstreet-sdk' }, head_repository: { full_name: 'Tealstreet/tealstreet-sdk' } };
  const pull = { number: 20, id: 21, state: 'open', head: { sha: H, repo: { full_name: 'Tealstreet/tealstreet-sdk', url: 'https://api.github.com/repos/Tealstreet/tealstreet-sdk' } },
    base: { sha: A, ref: 'master', repo: { full_name: 'Tealstreet/tealstreet-sdk', url: 'https://api.github.com/repos/Tealstreet/tealstreet-sdk' } } };
  run.pull_requests = [structuredClone(pull)];
  const jobs = [{ id: 30, runner_id: 40, run_id: 10, run_attempt: 1, head_sha: H,
    name: 'Rust / Bitget counterbalanced steady CPU' }];
  const pulls = [pull], calls = [];
  const state = { checkout: { sha: B, tree: { sha: TREE }, parents: [{ sha: A }, { sha: H }] },
    localParents: [A, H], firstParents: [A, BITGET_CUMULATIVE_ANCHOR], ancestor: true, retiredLanded: true, retiredFirstParents: [RETIRED_RUST_LANDING_TIP, BITGET_CUMULATIVE_ANCHOR], landingTip: A, localLanding: A, mergeTree: TREE, origin: 'https://github.com/Tealstreet/tealstreet-sdk' };
  const reader = { getRun: async () => run, listJobs: async () => jobs,
    getRustLanding: async () => ({ ref: 'refs/heads/master',
      url: 'https://api.github.com/repos/Tealstreet/tealstreet-sdk/git/refs/heads/master', object: { type: 'commit', sha: state.landingTip } }),
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
      'refs/heads/master:refs/remotes/origin/master']); return ''; }
    if (args[0] === 'rev-parse' && args[1] === 'refs/remotes/origin/master') return state.localLanding;
    if (args[0] === 'merge-base' && args[2] === RETIRED_RUST_LANDING_TIP) {
      assert.equal(args[3], state.landingTip);
      if (!state.retiredLanded) throw new Error('not an ancestor');
      return '';
    }
    if (args[0] === 'rev-list' && args[2] === RETIRED_RUST_LANDING_TIP) return state.retiredFirstParents.join('\n');
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

test('the anchor may sit on the retired landing chain that master merged', async () => {
  const x = fixture(); x.state.firstParents = [A];
  assert.equal((await selectBitgetPair(x.context)).baseline, A);
});

test('baseline comes from the authenticated checkout first parent on the actual landing chain', async () => {
  const x = fixture(), result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, B); assert.equal(result.head, H);
  assert.ok(x.calls.some(args => args[0] === 'merge-tree'));
});

function mergePull(pull) {
  pull.state = 'closed';
  pull.merged = true;
  pull.merged_at = '2026-11-05T20:18:39Z';
  pull.merge_commit_sha = 'e'.repeat(40);
}

test('an exact merged PR retains the authenticated original comparison', async () => {
  const x = fixture(), expected = await selectBitgetPair(x.context);
  mergePull(x.pull);
  assert.deepEqual(await selectBitgetPair(x.context), expected);
});

test('an exact merged PR with an empty run snapshot retains the original comparison', async () => {
  const x = fixture(), expected = await selectBitgetPair(x.context);
  mergePull(x.pull);
  x.run.pull_requests = [];
  assert.deepEqual(await selectBitgetPair(x.context), expected);
});

test('empty merged snapshots retain authenticated head repository fences', async t => {
  const cases = [
    ['fork pull head', x => { x.pull.head.repo.full_name = 'fork/tealstreet-sdk'; }],
    ['fork run head', x => { x.run.head_repository.full_name = 'fork/tealstreet-sdk'; }],
    ['missing pull head repository', x => { delete x.pull.head.repo; }],
    ['missing run head repository', x => { delete x.run.head_repository; }],
    ['missing snapshot inventory', x => { delete x.run.pull_requests; }],
    ['malformed snapshot inventory', x => { x.run.pull_requests = ''; }],
  ];
  for (const [name, mutate] of cases) await t.test(name, async () => {
    const x = fixture();
    mergePull(x.pull);
    x.run.pull_requests = [];
    mutate(x);
    await assert.rejects(selectBitgetPair(x.context));
  });
});

test('replay family selection accepts the same authenticated merged PR', async () => {
  const x = fixture();
  mergePull(x.pull);
  x.run.pull_requests = [];
  x.jobs[0].name = 'Rust / replay measurement (binance)';
  const result = await familyComparisonAuthority({ ...x.context, suite: 'binance' });
  assert.equal(result.baseline, A);
  assert.equal(result.candidate, B);
  assert.equal(result.candidateTree, TREE);
});

test('a PR merging between association and refresh retains the original comparison', async () => {
  const x = fixture();
  x.pulls[0] = structuredClone(x.pull);
  mergePull(x.pull);
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A);
  assert.equal(result.candidate, B);
  assert.equal(result.head, H);
});

test('closed unmerged associations do not compete with the exact open PR', async () => {
  const x = fixture();
  x.pulls.push({ ...structuredClone(x.pull), number: 99, id: 100, state: 'closed', merged_at: null });
  assert.equal((await selectBitgetPair(x.context)).pullNumber, 20);
});

test('merged selection retains ambiguity and refreshed source checks', async t => {
  const cases = [
    ['two eligible exact PRs', x => { x.pulls.push(structuredClone(x.pull)); }],
    ['closed without a merge', x => { x.pull.merged = false; }],
    ['missing merge time', x => { x.pull.merged_at = null; }],
    ['invalid merge time', x => { x.pull.merged_at = 'not a timestamp'; }],
    ['missing merge commit', x => { x.pull.merge_commit_sha = null; }],
    ['head changed after association', x => { x.pull.head.sha = A; }],
    ['foreign base after association', x => { x.pull.base.repo.full_name = 'fork/tealstreet-sdk'; }],
    ['present but mismatched run association', x => { x.run.pull_requests[0].head.sha = A; }],
    ['present duplicate run association', x => { x.run.pull_requests.push(structuredClone(x.run.pull_requests[0])); }],
  ];
  for (const [name, mutate] of cases) await t.test(name, async () => {
    const x = fixture();
    mergePull(x.pull);
    x.pulls[0] = structuredClone(x.pull);
    mutate(x);
    await assert.rejects(selectBitgetPair(x.context));
  });
});

test('ambiguous, stale, cross-branch or retried source/producer fails closed', async () => {
  for (const mutate of [x => x.pulls.push(x.pull), x => x.jobs.push(x.jobs[0]),
    x => { x.pulls.length = 0; }, x => { x.pull.state = 'closed'; }, x => { x.pull.base.ref = 'feat/other'; },
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
  const x = fixture(); x.run.event = 'push'; x.run.head_branch = 'master';
  x.context.gitRead = (...args) => args[1] === 'HEAD' ? H : TREE;
  x.context.reader.listCommitPulls = async () => { throw new Error('push must not select a PR baseline'); };
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, H);
});

test('a dispatched master run (nightly certification) uses the landed tip like a push', async () => {
  const x = fixture(); x.run.event = 'workflow_dispatch'; x.run.head_branch = 'master';
  x.context.gitRead = (...args) => args[1] === 'HEAD' ? H : TREE;
  x.context.reader.listCommitPulls = async () => { throw new Error('a master dispatch must not select a PR baseline'); };
  const result = await selectBitgetPair(x.context);
  assert.equal(result.baseline, A); assert.equal(result.candidate, H);
  const branch = fixture(); branch.run.event = 'workflow_dispatch'; branch.run.head_branch = 'feat/other';
  branch.context.reader.listCommitPulls = async () => [];
  await assert.rejects(selectBitgetPair(branch.context), /PR/);
});

test('non-landing branch pushes cannot obtain a Rust paired certificate', async () => {
  const x = fixture(); x.run.event = 'push'; x.run.head_branch = 'feat/other';
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
    ['fixed anchor is on neither the live nor the retired first-parent chain', x => {
      x.state.firstParents = [A]; x.state.retiredFirstParents = [RETIRED_RUST_LANDING_TIP]; }, /fixed cumulative anchor/],
    ['retired landing tip is not under the live landing', x => {
      x.state.firstParents = [A]; x.state.retiredLanded = false; }, /not an ancestor/],
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
        url: 'https://api.github.com/repos/fork/tealstreet-sdk/git/refs/heads/master' });
    }, /ref from the base repository/],
    ['API and local landing ref disagree', x => { x.state.localLanding = H; }, /API and local current Rust landing ref/],
    ['pull base repository is foreign', x => { x.pull.base.repo.full_name = 'fork/tealstreet-sdk'; }, /exact open or merged PR/],
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
      args[1] === 'refs/remotes/origin/master' ? x.state.landingTip : actualTree;
    if (args[0] === 'remote') return 'https://github.com/Tealstreet/tealstreet-sdk.git';
    if (args[0] === 'show') return `${landed} ${head}`;
    if (args[0] === 'fetch') { assert.equal(args[3], 'refs/heads/master:refs/remotes/origin/master'); return ''; }
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

test('run 40518541502: the paired producer is re-listed until GitHub lists it with its runner', async () => {
  const relisted = async (...listings) => {
    const x = fixture(), sleeps = [], producer = x.jobs[0];
    x.context.reader.listJobs = async () => (listings.length > 1 ? listings.shift() : listings[0])(producer);
    x.context.sleep = async ms => { sleeps.push(ms); };
    return { x, sleeps, result: selectBitgetPair(x.context) };
  };
  for (const listings of [[p => [{ ...p, runner_id: 0 }], p => [{ ...p, runner_id: null }], p => [p]], [() => [], p => [p]]]) {
    const { sleeps, result } = await relisted(...listings);
    assert.equal((await result).runnerId, '40'); assert.deepEqual(sleeps, Array(listings.length - 1).fill(10_000));
  }
  const unassigned = await relisted(p => [{ ...p, runner_id: 0, status: 'in_progress' }]);
  await assert.rejects(unassigned.result,
    /missing\/duplicate paired physical producer; candidates \[\{"id":30,"name":"Rust \/ Bitget counterbalanced steady CPU","run_attempt":1,"runner_id":0,"status":"in_progress"\}\]/);
  assert.equal(unassigned.sleeps.length, 6);
  const duplicate = await relisted(p => [p, p]);
  await assert.rejects(duplicate.result, /missing\/duplicate paired physical producer/);
  assert.equal(duplicate.sleeps.length, 0);
});

test('replay checkout preserves real merge parents after a branch update from master', async t => {
  const fs = await import('node:fs');
  const { execFileSync } = await import('node:child_process');
  const { tmpdir } = await import('node:os');
  const { resolve } = await import('node:path');
  const { pathToFileURL } = await import('node:url');
  const workflow = fs.readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const gate = /^  gate:\n([\s\S]*?)(?=^  [a-z][a-z0-9-]*:\n)/m.exec(workflow)?.[1];
  assert(gate, 'missing replay measurement job');
  assert.match(gate, /uses: actions\/checkout@v7\n        with:\n          ref: \$\{\{ github\.sha \}\}\n          fetch-depth: 1\n          fetch-tags: false\n      - uses: \.\/\.github\/actions\/fetch-rust-history\n(?=      - )/);
  const historyHelper = resolve('.github/actions/fetch-rust-history/fetch-history.py');
  const root = fs.realpathSync(fs.mkdtempSync(resolve(tmpdir(), 'cpu-checkout-history-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const origin = resolve(root, 'origin'); fs.mkdirSync(origin);
  const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git(origin, 'init', '-q', '-b', 'master'); git(origin, 'config', 'user.name', 'Fixture'); git(origin, 'config', 'user.email', 'fixture@example.invalid');
  fs.writeFileSync(resolve(origin, 'base'), 'base'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'base');
  git(origin, 'switch', '-qc', 'fix'); fs.writeFileSync(resolve(origin, 'own'), 'own'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'own');
  git(origin, 'switch', '-q', 'master'); fs.writeFileSync(resolve(origin, 'incoming'), 'incoming'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'incoming');
  const base = git(origin, 'rev-parse', 'HEAD');
  git(origin, 'switch', '-q', 'fix'); git(origin, 'merge', '--no-ff', '-qm', 'update branch', 'master');
  const head = git(origin, 'rev-parse', 'HEAD');
  git(origin, 'switch', '-qc', 'merge-checkout', base); git(origin, 'merge', '--no-ff', '-qm', 'PR checkout', 'fix');
  const checkout = git(origin, 'rev-parse', 'HEAD'), tree = git(origin, 'rev-parse', 'HEAD^{tree}');
  const url = pathToFileURL(origin).href;
  const clone = (name, depth) => {
    const directory = resolve(root, name);
    git(root, 'clone', '-q', '--no-checkout', '--branch', 'merge-checkout', ...(depth ? ['--depth', String(depth)] : []), url, directory);
    return directory;
  };
  const shallow = clone('working-control-shallow', 1);
  assert.equal(git(shallow, 'show', '-s', '--format=%P', 'HEAD'), '', 'depth-one Git hides both parents');
  assert.deepEqual(git(shallow, 'cat-file', '-p', 'HEAD').split('\n').filter(line => line.startsWith('parent ')).map(line => line.slice(7)), [base, head]);
  const actual = resolve(root, 'configured-checkout'); fs.mkdirSync(actual);
  git(actual, 'init', '-q', '-b', 'master'); git(actual, 'remote', 'add', 'origin', url);
  git(actual, 'fetch', '--no-tags', '--depth=1', 'origin', checkout);
  git(actual, 'checkout', '-q', '--detach', checkout);
  assert.equal(git(actual, 'show', '-s', '--format=%P', 'HEAD'), '', 'exact-SHA checkout begins shallow');
  const eventPath = resolve(root, 'event.json');
  fs.writeFileSync(eventPath, JSON.stringify({ pull_request: {
    base: { sha: base, ref: 'master' }, head: { sha: head },
  } }));
  execFileSync('python3', [historyHelper], { cwd: actual, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, GITHUB_SHA: checkout, GITHUB_EVENT_NAME: 'pull_request', GITHUB_EVENT_PATH: eventPath } });
  assert.equal(git(actual, 'rev-parse', '--is-shallow-repository'), 'false');
  assert.equal(git(actual, 'rev-parse', 'HEAD'), checkout);
  assert.equal(git(actual, 'rev-parse', 'HEAD^{tree}'), tree);
  assert.deepEqual(git(actual, 'show', '-s', '--format=%P', 'HEAD').split(' '), [base, head],
    'workflow checkout must expose both authenticated PR parents to the shared selector');
});
