import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { createGithubReader } from './cpu-cohort.mjs';
import { readBitgetPairedEvidence } from './bitget-paired-artifacts.mjs';
import { BITGET_PAIRED_FREEZE_SHA256, BITGET_PAIRED_PROTOCOL_SHA256, BITGET_MEASUREMENT_FILES, BITGET_CUMULATIVE_ANCHOR,
  assessBitgetPairedSteady, bitgetPairedAuthority, bitgetAnchorAuthority,
  combineBitgetPairedCertificates } from './bitget-paired-steady.mjs';

const insist = (ok, message) => { if (!ok) throw new Error(`Bitget paired CI: ${message}`); };
const git = (root, ...args) => {
  const child = spawnSync('git', args, { cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  insist(child.status === 0 && !child.error && !child.signal, `git ${args[0]} failed: ${child.stderr ?? ''}`);
  return child.stdout.trim();
};
// #2299 merged the retired feat/rust-rewrite into master as a second parent, so the
// anchor sits on that frozen tip's first-parent chain rather than master's.
export const RETIRED_RUST_LANDING_TIP = '9f5ea60ecd4b2430b279a73f5e0d9fbfd9869f14';
const retiredLandingHolds = (gitRead, landingTip) =>
  gitRead('merge-base', '--is-ancestor', RETIRED_RUST_LANDING_TIP, landingTip) === '' &&
  gitRead('rev-list', '--first-parent', RETIRED_RUST_LANDING_TIP).split('\n').includes(BITGET_CUMULATIVE_ANCHOR);
const write = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });

/** Select from authenticated run/PR/commit APIs, never an input baseline flag.
 * The Git tree is recomputed before measuring, not accepted from the PR. */
export async function selectBitgetPair({ reader, runId, root, gitRead = (...args) => git(root, ...args) }) {
  const run = await reader.getRun(runId), jobs = await reader.listJobs(runId);
  const candidates = jobs.filter(job => job.name === 'Rust / Bitget counterbalanced steady CPU');
  insist(candidates.length === 1, 'missing/duplicate paired physical producer');
  const checkoutSha = gitRead('rev-parse', 'HEAD'), checkoutTree = gitRead('rev-parse', 'HEAD^{tree}');
  let pull, landing, mergeTree;
  if (run.event === 'push' && run.head_branch === 'master') {
    landing = await reader.getCommit(run.head_sha); mergeTree = checkoutTree;
  } else {
    const associated = await reader.listCommitPulls(run.head_sha);
    const matching = associated.filter(item => item.state === 'open' && item.head?.sha === run.head_sha &&
      item.base?.ref === 'master' && item.base?.repo?.full_name === 'Tealstreet/tealstreet-sdk');
    insist(matching.length === 1, 'missing/ambiguous exact open PR for this run');
    pull = await reader.getPull(matching[0].number);
    insist(pull.state === 'open' && pull.head?.sha === run.head_sha &&
      pull.base?.ref === 'master' && pull.base.repo?.full_name === 'Tealstreet/tealstreet-sdk' &&
      run.repository.full_name === 'Tealstreet/tealstreet-sdk', 'live PR must name the authenticated Rust landing base');
    let associatedBase;
    if (run.event === 'pull_request') {
      const snapshots = run.pull_requests?.filter(item => item.number === pull.number && item.head?.sha === run.head_sha);
      insist(snapshots?.length === 1 && snapshots[0].base?.ref === pull.base.ref &&
        snapshots[0].base.repo?.url === `https://api.github.com/repos/${run.repository.full_name}` &&
        snapshots[0].head.repo?.url === `https://api.github.com/repos/${run.repository.full_name}`,
      'missing exact run PR association');
      associatedBase = snapshots[0].base.sha;
    }
    const checkout = await reader.getCommit(checkoutSha);
    const localParents = gitRead('show', '-s', '--format=%P', checkoutSha).split(' ');
    const apiParents = checkout.parents?.map(parent => parent.sha);
    insist(checkout.sha === checkoutSha && checkout.tree?.sha === checkoutTree &&
      apiParents?.length === 2 && localParents.length === 2 && isDeepStrictEqual(apiParents, localParents) &&
      apiParents.every(parent => /^[a-f0-9]{40}$/.test(parent)) && apiParents[1] === run.head_sha,
    'checkout API/local tree and exact two-parent PR composition must agree');
    const baseline = apiParents[0], landingRef = await reader.getRustLanding();
    insist(landingRef.ref === 'refs/heads/master' &&
      landingRef.url === 'https://api.github.com/repos/Tealstreet/tealstreet-sdk/git/refs/heads/master' &&
      landingRef.object?.type === 'commit' && /^[a-f0-9]{40}$/.test(landingRef.object.sha),
    'authenticated current Rust landing ref from the base repository required');
    const landingTip = landingRef.object.sha;
    insist(/^https:\/\/github\.com\/Tealstreet\/tealstreet-sdk(?:\.git)?$/.test(gitRead('remote', 'get-url', 'origin')),
      'local origin must be the authenticated base repository');
    gitRead('fetch', '--no-tags', 'origin', 'refs/heads/master:refs/remotes/origin/master');
    insist(gitRead('rev-parse', 'refs/remotes/origin/master') === landingTip,
      'API and local current Rust landing ref must agree');
    const firstParents = gitRead('rev-list', '--first-parent', landingTip).split('\n');
    insist(gitRead('merge-base', '--is-ancestor', baseline, landingTip) === '' && firstParents.includes(baseline),
    'checkout base must be an actual first-parent Rust landing');
    insist(firstParents.includes(BITGET_CUMULATIVE_ANCHOR) || retiredLandingHolds(gitRead, landingTip),
      'fixed cumulative anchor must be on the live first-parent landing chain');
    let graphqlBaseRefOid = null, graphqlError = null;
    try {
      graphqlBaseRefOid = await reader.getPullBaseOid(pull.number);
    } catch (error) {
      graphqlError = String(error?.message ?? error);
    }
    console.log(JSON.stringify({ bitgetBaseSelection: { checkout: checkoutSha, baseline, landingTip,
      runAssociatedBase: associatedBase ?? null, pullMetadataBase: pull.base.sha ?? null,
      graphqlBaseRefOid: graphqlBaseRefOid ?? null, graphqlError } }));
    pull = { ...pull, base: { ...pull.base, sha: baseline } };
    mergeTree = gitRead('merge-tree', '--write-tree', baseline, pull.head.sha);
    insist(/^[a-f0-9]{40}$/.test(mergeTree), 'composition is not a clean automatic tree');
  }
  const measurementFiles = Object.fromEntries(BITGET_MEASUREMENT_FILES.map(path => [path, gitRead('rev-parse', `${checkoutSha}:${path}`)]));
  return bitgetPairedAuthority({ run, pull, landing, job: candidates[0], checkoutSha, checkoutTree, mergeTree, measurementFiles });
}

export async function collectBitgetPair({ reader, runId, root, destination, env = process.env }) {
  insist(env.GITHUB_ACTIONS === 'true' && env.GITHUB_RUN_ATTEMPT === '1' &&
    String(runId) === env.GITHUB_RUN_ID, 'measurement requires original Actions attempt1');
  const authority = await selectBitgetPair({ reader, runId, root });
  mkdirSync(destination, { recursive: false });
  const script = resolve(destination, 'collect-bitget-paired.py');
  copyFileSync(resolve(root, 'sdk-rs/harness/collect-bitget-paired.py'), script);
  const witnesses = { tip: authority, anchor: bitgetAnchorAuthority(authority) };
  for (const [name, selected] of Object.entries(witnesses)) {
    const directory = resolve(destination, name); mkdirSync(directory);
    const authorityPath = resolve(directory, 'authority.json');
    write(authorityPath, { ...selected, protocolSha256: BITGET_PAIRED_PROTOCOL_SHA256,
      freezeSha256: BITGET_PAIRED_FREEZE_SHA256 });
    try {
      const child = spawnSync('python3', [script, '--baseline', selected.baseline,
        '--candidate', selected.candidate, '--authority', authorityPath, '--out', resolve(directory, 'records'),
        '--module-path', resolve(destination, 'active-module')], { cwd: root, env, stdio: 'inherit' });
      insist(child.status === 0 && !child.error && !child.signal, `${name} fixed measurement did not complete; no retry`);
    } finally {
      git(root, 'checkout', '--detach', authority.candidate);
    }
  }
  const current = await selectBitgetPair({ reader, runId, root });
  insist(isDeepStrictEqual(current, authority), 'source/base/physical producer changed during measurement');
  const manifest = JSON.parse(readFileSync(resolve(root, 'sdk-rs/harness/known-gate-failures.json'), 'utf8'));
  const measured = Object.fromEntries(Object.entries(witnesses).map(([name, selected]) => {
    const result = assessBitgetPairedSteady({ authority: selected,
      evidence: readBitgetPairedEvidence(resolve(destination, name, 'records')), manifest });
    insist(result.valid, result.errors.join('\n')); return [name, result.certificate];
  }));
  const certificate = combineBitgetPairedCertificates(measured.tip, measured.anchor);
  const result = { valid: true, errors: [], certificate };
  write(resolve(destination, 'steady-assessment.json'), result);
  console.log(JSON.stringify({ tipQ: certificate.tip.q, anchorQ: certificate.anchor.q, tolerance: certificate.tolerance,
    steadyAccepted: certificate.steadyAccepted, readinessByCpuModel: certificate.readinessByCpuModel,
    otherObjectivesRemainUnchanged: true }));
  insist(result.certificate.steadyAccepted, 'counterbalanced non-regression bound exceeded');
  return result;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { out: { type: 'string' } }, strict: true });
  insist(values.out && process.env.GITHUB_TOKEN && process.env.GITHUB_REPOSITORY === 'Tealstreet/tealstreet-sdk',
    'missing original read-only Actions context/output');
  const root = realpathSync(process.cwd()), destination = resolve(values.out);
  mkdirSync(dirname(destination), { recursive: true });
  const reader = createGithubReader({ repository: process.env.GITHUB_REPOSITORY, token: process.env.GITHUB_TOKEN });
  await collectBitgetPair({ reader, runId: Number(process.env.GITHUB_RUN_ID), root, destination });
}
