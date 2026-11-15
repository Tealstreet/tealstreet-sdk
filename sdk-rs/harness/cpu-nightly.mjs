// Nightly CPU certification (Sam, 2026-11-07): a suite whose CPU gate failed on two consecutive
// UTC nights' runs of rust.yml opens or updates one cpu-regression issue.
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

export const NIGHTLY_ACTOR = 'github-actions[bot]';
export const ISSUE_LABEL = 'cpu-regression';
export const ISSUE_TITLE = 'CPU regression on master (nightly certification)';
const CPU_GATE = /^Rust \/ CPU gate \(([a-z0-9-]+)\)$/;

const utcNight = (run) => run.created_at.slice(0, 10);

/** One result per UTC night, newest first: the last completed first-attempt run the nightly
 * workflow dispatched on master that night. Extra same-night runs never count twice. */
export function nightlyRuns(runs) {
  const nights = new Map();
  for (const run of runs
    .filter((run) => run.event === 'workflow_dispatch' && run.head_branch === 'master' && run.status === 'completed' &&
      run.triggering_actor?.login === NIGHTLY_ACTOR && run.run_attempt === 1 && Number.isFinite(Date.parse(run.created_at)))
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at) || b.id - a.id))
    if (!nights.has(utcNight(run))) nights.set(utcNight(run), run);
  return [...nights.values()];
}

/** The two runs are on adjacent UTC calendar nights. */
export function consecutiveNights(latest, previous) {
  return Boolean(latest && previous) &&
    Date.parse(`${utcNight(latest)}T00:00:00Z`) - Date.parse(`${utcNight(previous)}T00:00:00Z`) === 86_400_000;
}

/** Suites whose CPU gate failed in one run's job inventory. */
export function failedCpuGates(jobs) {
  return [...new Set(jobs.filter((job) => job.conclusion === 'failure').map((job) => CPU_GATE.exec(job.name)?.[1]).filter(Boolean))].sort();
}

/** Failed on both of the two latest nights; fewer than two nights is never sustained. */
export function sustainedRegressions(latest, previous) {
  if (!latest || !previous) return [];
  const before = new Set(previous);
  return latest.filter((suite) => before.has(suite));
}

export function issueBody({ suites, runs }) {
  return [
    'Rust was slower than TS (> 1.00x after the one in-job re-measure), or CPU certification failed, for these suites on two consecutive nightly runs:',
    '',
    ...suites.map((suite) => `- \`${suite}\``),
    '',
    ...runs.map((run) => `- Run ${run.id} (${run.head_sha.slice(0, 11)}, ${run.created_at}): ${run.html_url}`),
    '',
    'Policy: Sam, 2026-11-07. Rust must not be slower than TS; nightly certification files cpu-regression issues.',
  ].join('\n');
}

function github(token, repository) {
  const request = async (path, init = {}) => {
    const response = await fetch(`https://api.github.com/repos/${repository}${path}`, { ...init,
      headers: { accept: 'application/vnd.github+json', authorization: `Bearer ${token}`, 'x-github-api-version': '2022-11-28', ...init.headers } });
    if (!response.ok) throw new Error(`GitHub ${init.method ?? 'GET'} ${path}: ${response.status} ${await response.text()}`);
    return response.status === 204 ? null : response.json();
  };
  return {
    request,
    async jobs(runId) {
      const jobs = [];
      for (let page = 1; ; page++) {
        const batch = (await request(`/actions/runs/${runId}/jobs?filter=latest&per_page=100&page=${page}`)).jobs;
        jobs.push(...batch);
        if (batch.length < 100) return jobs;
      }
    },
  };
}

export async function evaluate({ token, repository, log = console.log }) {
  const api = github(token, repository);
  const runs = [];
  let nights;
  // Manual dispatches and extra same-night runs can fill entire API pages.
  for (let page = 1; ; page++) {
    const batch = (await api.request(`/actions/workflows/rust.yml/runs?event=workflow_dispatch&branch=master&status=completed&per_page=50&page=${page}`)).workflow_runs;
    runs.push(...batch);
    nights = nightlyRuns(runs);
    if (nights.length >= 2 || batch.length < 50) break;
  }
  const [latest, previous] = nights;
  if (!latest || !previous) return log('Fewer than two completed nightly runs; nothing to compare.');
  if (!consecutiveNights(latest, previous))
    return log(`Nights ${utcNight(previous)} and ${utcNight(latest)} are not consecutive; nothing is sustained.`);
  const suites = sustainedRegressions(failedCpuGates(await api.jobs(latest.id)), failedCpuGates(await api.jobs(previous.id)));
  if (!suites.length) return log(`No CPU gate failed on both run ${latest.id} and run ${previous.id}.`);
  const body = issueBody({ suites, runs: [latest, previous] });
  const open = (await api.request(`/issues?state=open&labels=${ISSUE_LABEL}&per_page=100`)).filter((issue) => !issue.pull_request);
  if (open.length) {
    await api.request(`/issues/${open[0].number}/comments`, { method: 'POST', body: JSON.stringify({ body }) });
    log(`::warning title=CPU regression::Updated issue #${open[0].number}: ${suites.join(', ')}`);
  } else {
    const issue = await api.request('/issues', { method: 'POST', body: JSON.stringify({ title: ISSUE_TITLE, body, labels: [ISSUE_LABEL] }) });
    log(`::warning title=CPU regression::Opened issue #${issue.number}: ${suites.join(', ')}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { command: { type: 'string' } } });
  if (values.command !== 'evaluate') throw new Error('usage: cpu-nightly.mjs --command evaluate');
  await evaluate({ token: process.env.GITHUB_TOKEN, repository: process.env.GITHUB_REPOSITORY });
}
