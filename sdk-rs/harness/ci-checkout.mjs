import { isDeepStrictEqual } from 'node:util';

const repository = 'Tealstreet/tealstreet-sdk';
const oid = value => typeof value === 'string' && /^[a-f0-9]{40}$/.test(value);
const insist = (ok, message) => { if (!ok) throw new Error(`CI checkout: ${message}`); };

// A manual fresh run must measure the same PR composition as an automatic run,
// not a one-parent feature head. Neither a ref input nor metadata base selects A.
export async function selectDispatchedCheckout({ reader, runId, git }) {
  const run = await reader.getRun(runId);
  insist(run.id === runId && run.run_attempt === 1 && run.event === 'workflow_dispatch' &&
    run.repository?.full_name === repository && oid(run.head_sha), 'original repository dispatch required');
  insist(git('rev-parse', 'HEAD') === run.head_sha, 'initial checkout differs from dispatched head');
  if (run.head_branch === 'feat/rust-rewrite') return run.head_sha;
  const matches = (await reader.listCommitPulls(run.head_sha)).filter(pull => pull.state === 'open' &&
    pull.head?.sha === run.head_sha && pull.base?.ref === 'feat/rust-rewrite' && pull.base?.repo?.full_name === repository);
  insist(matches.length === 1, 'missing/ambiguous exact Rust PR');
  const pull = await reader.getPull(matches[0].number);
  insist(pull.number === matches[0].number && pull.state === 'open' && pull.head?.sha === run.head_sha &&
    pull.head?.repo?.full_name === repository && pull.base?.ref === 'feat/rust-rewrite' &&
    pull.base?.repo?.full_name === repository, 'live PR association changed');
  insist(/^https:\/\/github\.com\/Tealstreet\/tealstreet-sdk(?:\.git)?$/.test(git('remote', 'get-url', 'origin')),
    'origin is not the base repository');
  const ref = `refs/pull/${pull.number}/merge`;
  git('fetch', '--no-tags', 'origin', ref);
  const candidate = git('rev-parse', 'FETCH_HEAD'), commit = await reader.getCommit(candidate);
  const parents = git('show', '-s', '--format=%P', candidate).split(' '), tree = git('rev-parse', `${candidate}^{tree}`);
  insist(oid(candidate) && commit.sha === candidate && commit.tree?.sha === tree &&
    parents.length === 2 && parents.every(oid) && parents[1] === run.head_sha &&
    isDeepStrictEqual(parents, commit.parents?.map(parent => parent.sha)), 'API/local PR composition disagrees');
  const landing = await reader.getRustLanding();
  insist(landing.ref === 'refs/heads/feat/rust-rewrite' && landing.object?.type === 'commit' && oid(landing.object.sha) &&
    landing.url === 'https://api.github.com/repos/Tealstreet/tealstreet-sdk/git/refs/heads/feat/rust-rewrite', 'base-repository landing ref required');
  git('fetch', '--no-tags', 'origin', 'refs/heads/feat/rust-rewrite:refs/remotes/origin/feat/rust-rewrite');
  insist(git('rev-parse', 'refs/remotes/origin/feat/rust-rewrite') === landing.object.sha &&
    git('rev-list', '--first-parent', landing.object.sha).split('\n').includes(parents[0]), 'checkout base is not a live first-parent landing');
  insist(git('merge-tree', '--write-tree', ...parents) === tree, 'checkout is not the exact automatic merge');
  git('checkout', '--detach', candidate);
  return candidate;
}
