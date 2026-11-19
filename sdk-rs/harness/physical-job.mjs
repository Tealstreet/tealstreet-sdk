// GitHub can list a started job before it records runner_id, or list it late.
// Only those two states are re-listed, a bounded number of times; callers keep
// their own identity checks, so anything still missing after the bound fails.
// #3640 / CI44248450357: multiple started measurements still listed runner_id=0
// after 60 seconds; their terminal API records contain the assigned positive ID.
// Allow five minutes for metadata propagation, retaining every identity refusal.
export const PHYSICAL_JOB_RELISTS = 30;
export const PHYSICAL_JOB_RELIST_MS = 10_000;

const awaitingIdentity = selected => selected.length === 0 ||
  (selected.length === 1 && !(Number.isSafeInteger(selected[0].runner_id) && selected[0].runner_id > 0));

export async function relistPhysicalJob(load, name, { sleep = ms => new Promise(resolve => setTimeout(resolve, ms)),
  relists = PHYSICAL_JOB_RELISTS, delayMs = PHYSICAL_JOB_RELIST_MS } = {}) {
  for (let relist = 0; ; relist++) {
    const listing = await load(relist > 0), selected = listing.jobs.filter(job => job.name === name);
    if (!awaitingIdentity(selected) || relist === relists) return { ...listing, selected };
    await sleep(delayMs);
  }
}

export const physicalCandidateRows = selected => JSON.stringify(selected.map(job =>
  ({ id: job.id, name: job.name, run_attempt: job.run_attempt, runner_id: job.runner_id ?? null, status: job.status })));
