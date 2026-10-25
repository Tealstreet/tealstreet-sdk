import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { createGithubReader } from './cpu-cohort.mjs';
import { readBitgetPairedEvidence } from './bitget-paired-artifacts.mjs';
import { BITGET_PAIRED_FREEZE_SHA256, BITGET_PAIRED_PROTOCOL_SHA256,
  assessBitgetPairedSteady, bitgetPairedAuthority } from './bitget-paired-steady.mjs';

const insist = (ok, message) => { if (!ok) throw new Error(`Bitget paired CI: ${message}`); };
const git = (root, ...args) => {
  const child = spawnSync('git', args, { cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  insist(child.status === 0 && !child.error && !child.signal, `git ${args[0]} failed: ${child.stderr ?? ''}`);
  return child.stdout.trim();
};
const write = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });

/** Select from authenticated run/PR/commit APIs, never an input baseline flag.
 * The Git tree is recomputed before measuring, not accepted from the PR. */
export async function selectBitgetPair({ reader, runId, root, gitRead = (...args) => git(root, ...args) }) {
  const run = await reader.getRun(runId), jobs = await reader.listJobs(runId);
  const candidates = jobs.filter(job => job.name === 'Rust / Bitget counterbalanced steady CPU');
  insist(candidates.length === 1, 'missing/duplicate paired physical producer');
  const checkoutSha = gitRead('rev-parse', 'HEAD'), checkoutTree = gitRead('rev-parse', 'HEAD^{tree}');
  let pull, landing, mergeTree;
  if (run.event === 'push' && ['feat/rust-rewrite', 'master'].includes(run.head_branch)) {
    landing = await reader.getCommit(run.head_sha); mergeTree = checkoutTree;
  } else {
    const associated = await reader.listCommitPulls(run.head_sha);
    const matching = associated.filter(item => item.state === 'open' && item.head?.sha === run.head_sha &&
      item.base?.ref === 'feat/rust-rewrite' && item.base?.repo?.full_name === 'Tealstreet/tealstreet-sdk');
    insist(matching.length === 1, 'missing/ambiguous exact open PR for this run');
    pull = await reader.getPull(matching[0].number);
    if (run.event === 'pull_request') {
      const snapshots = run.pull_requests?.filter(item => item.number === pull.number && item.head?.sha === run.head_sha);
      insist(snapshots?.length === 1 && snapshots[0].base?.ref === pull.base.ref &&
        /^[a-f0-9]{40}$/.test(snapshots[0].base.sha), 'missing original run base snapshot');
      pull = { ...pull, base: { ...pull.base, sha: snapshots[0].base.sha } };
    }
    mergeTree = gitRead('merge-tree', '--write-tree', pull.base.sha, pull.head.sha);
    insist(/^[a-f0-9]{40}$/.test(mergeTree), 'composition is not a clean automatic tree');
  }
  return bitgetPairedAuthority({ run, pull, landing, job: candidates[0], checkoutSha, checkoutTree, mergeTree });
}

export async function collectBitgetPair({ reader, runId, root, destination, env = process.env }) {
  insist(env.GITHUB_ACTIONS === 'true' && env.GITHUB_RUN_ATTEMPT === '1' &&
    String(runId) === env.GITHUB_RUN_ID, 'measurement requires original Actions attempt1');
  const authority = await selectBitgetPair({ reader, runId, root });
  mkdirSync(destination, { recursive: false });
  const authorityPath = resolve(destination, 'authority.json'), script = resolve(destination, 'collect-bitget-paired.py');
  write(authorityPath, { ...authority, protocolSha256: BITGET_PAIRED_PROTOCOL_SHA256,
    freezeSha256: BITGET_PAIRED_FREEZE_SHA256 });
  copyFileSync(resolve(root, 'sdk-rs/harness/collect-bitget-paired.py'), script);
  const records = resolve(destination, 'records');
  try {
    const child = spawnSync('python3', [script, '--baseline', authority.baseline,
      '--candidate', authority.candidate, '--authority', authorityPath, '--out', records],
    { cwd: root, env, stdio: 'inherit' });
    insist(child.status === 0 && !child.error && !child.signal, 'fixed measurement did not complete; no retry');
  } finally {
    git(root, 'checkout', '--detach', authority.candidate);
  }
  const current = await selectBitgetPair({ reader, runId, root });
  insist(isDeepStrictEqual(current, authority), 'source/base/physical producer changed during measurement');
  const evidence = readBitgetPairedEvidence(records);
  const manifest = JSON.parse(readFileSync(resolve(root, 'sdk-rs/harness/known-gate-failures.json'), 'utf8'));
  const result = assessBitgetPairedSteady({ authority: current, evidence, manifest });
  write(resolve(destination, 'steady-assessment.json'), result);
  insist(result.valid, result.errors.join('\n'));
  console.log(JSON.stringify({ q: result.certificate.q, tolerance: result.certificate.tolerance,
    steadyAccepted: result.certificate.steadyAccepted, readinessByCpuModel: result.certificate.readiness,
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
