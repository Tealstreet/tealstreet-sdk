import { appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createGithubReader } from './cpu-cohort.mjs';

export function assessCiResult({ pullRequest, selection, jobs }) {
  if (!selection || typeof selection.full !== 'boolean' || typeof selection.nativeFull !== 'boolean' ||
      typeof selection.replayRequired !== 'boolean' || typeof selection.sizeRequired !== 'boolean' ||
      typeof selection.combinedRequired !== 'boolean')
    throw new Error('Missing authenticated coverage selection');
  if (selection.full && (!selection.nativeFull || !selection.replayRequired || !selection.sizeRequired || !selection.combinedRequired))
    throw new Error('Full coverage cannot omit native, replay or size checks');
  const required = ['selection', 'bans', 'harness', 'native'];
  if (selection.nativeFull) required.push('wasm');
  if (selection.combinedRequired || !pullRequest) required.push('combined');
  if (selection.replayRequired) required.push('gate');
  if (selection.sizeRequired) required.push('families');
  if (!pullRequest) required.push('browser', 'bitget-paired', 'cpu-plan', 'cpu-certify', 'cpu-final');
  const failures = required.filter(name => jobs?.[name]?.result !== 'success');
  return { required, failures, accepted: failures.length === 0 };
}

export async function landingOwnership({ reader, sha, actor }) {
  try {
    const pulls = await reader.listCommitPulls(sha);
    const introduced = pulls.find(pull => pull.merge_commit_sha === sha);
    if (introduced) return `Fix/revert owner: @${introduced.user.login}, PR #${introduced.number}, branch ${introduced.head.ref}.`;
  } catch (error) { console.error(`Landing owner lookup failed: ${error.message}`); }
  return `Fix/revert owner: @${actor}, landing ${sha}.`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const needs = JSON.parse(process.env.CI_JOB_RESULTS), selection = Object.fromEntries(
    ['full', 'nativeFull', 'replayRequired', 'sizeRequired', 'combinedRequired'].map(name => [name, JSON.parse(needs.selection?.outputs?.[name] ?? 'null')]));
  const pullRequest = process.env.GITHUB_EVENT_NAME === 'pull_request';
  let result;
  try { result = assessCiResult({ pullRequest, selection, jobs: needs }); }
  catch (error) { result = { accepted: false, failures: [error.message] }; }
  let ownership = '';
  if (!pullRequest && !result.accepted) {
    const reader = createGithubReader({ repository: process.env.GITHUB_REPOSITORY, token: process.env.GITHUB_TOKEN });
    ownership = await landingOwnership({ reader, sha: process.env.GITHUB_SHA, actor: process.env.GITHUB_ACTOR });
    console.error(`${ownership} Immediate fix or revert required; full landing verification is red.`);
  }
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY,
    `## Rust ${pullRequest ? 'PR' : 'landing'} checks\n\n${result.accepted ? 'PASS' : `FAIL: ${result.failures.join(', ')}`}\n\n${ownership}\n`);
  console.log(JSON.stringify(result));
  if (!result.accepted) process.exitCode = 1;
}
