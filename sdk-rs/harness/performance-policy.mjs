import { cpuObjectiveMet, SMALL_OBJECTIVE_MS } from './cpu-objective.mjs';

export const MIN_PUBLIC_RUNS = 7;
export const REQUIRED_IMPROVEMENT = 0.10;
export const performanceOrder = (run) => run % 2 === 0 ? ['ts', 'rust'] : ['rust', 'ts'];
export const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

export function replayRunCount(raw, isPrivate) {
  const count = Number(raw ?? (isPrivate ? 1 : MIN_PUBLIC_RUNS));
  if (!Number.isSafeInteger(count) || count < (isPrivate ? 1 : MIN_PUBLIC_RUNS))
    throw new Error(isPrivate ? 'private runs must be a positive integer' : `public performance requires at least ${MIN_PUBLIC_RUNS} interleaved runs`);
  return count;
}

export function performanceMetricErrors(metrics, mode) {
  const errors = [];
  if (metrics?.baselineKind !== 'idle-loaded') errors.push('missing idle-loaded memory baseline');
  if (metrics?.measuredMode !== mode) errors.push(`invalid measured mode (expected ${mode})`);
  for (const field of ['cpuMs', 'steadyCpuMs', 'processCpuMs']) {
    if (!Number.isFinite(metrics?.[field]) || metrics[field] <= 0) errors.push(`invalid ${field} (${metrics?.[field]})`);
  }
  return errors;
}

export function performanceFailures(ts, rust) {
  const errors = [];
  for (const [field, label] of [
    ['cpuMs', 'main-thread CPU'], ['steadyCpuMs', 'steady-state CPU'],
    ['processCpuMs', 'process CPU'], ['retainedBytes', 'retained memory'],
  ]) {
    const reference = median(ts.map((sample) => sample[field]));
    const candidate = median(rust.map((sample) => sample[field]));
    if (!Number.isFinite(reference) || reference <= 0 || !Number.isFinite(candidate))
      errors.push(`performance ${label}: no valid positive TS baseline`);
    // Sub-10 ms main/steady objectives are within run-to-run noise of a tie.
    else if (['cpuMs', 'steadyCpuMs'].includes(field) && reference < SMALL_OBJECTIVE_MS) {
      if (!cpuObjectiveMet(candidate, reference))
        errors.push(`performance ${label}: Rust must be within 1 ms of a sub-10 ms TS objective (ts=${reference}, rust=${candidate})`);
    } else if (!(candidate <= reference * (1 - REQUIRED_IMPROVEMENT)))
      errors.push(`performance ${label}: Rust must be at least 10% below TS (ts=${reference}, rust=${candidate})`);
  }
  return errors;
}
