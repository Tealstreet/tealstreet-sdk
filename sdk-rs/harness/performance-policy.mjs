// The replay gate's timing policy, in one place.
//
// Public tapes run at least seven interleaved TS/Rust pairs, alternating which
// side runs first, and compare per-metric medians. Rust must be at least 10%
// below TS on main-thread, steady-state and process CPU and on retained memory.
//
// Small windows: main-thread and steady CPU objectives whose TS median is under
// 100 ms are tens of ms of mostly shared replay work, where tied runs of the
// same code move several percent between CI runners (docs/plans/
// RUST_GATE_TIMING.md: Hyperliquid Spot steady 0.73-1.08 Rust/TS across runs
// of unchanged code). They get nine pairs and pass when Rust is at most
// max(1 ms, 5%) above TS; this subsumes #1380's sub-10 ms 1 ms allowance.
// Process CPU and memory keep the 10% rule at every size. A per-message
// slowdown on Bybit and Hyperliquid still fails these rules (PR #1461).
// Overseer decision (2026-10-21): the small-window floor replaces the 10% rule
// for those objectives; it cannot catch a regression that keeps Rust within 5%
// of TS on a small window. Revisit if one slips through.

export const MIN_PUBLIC_RUNS = 7;
export const REQUIRED_IMPROVEMENT = 0.10;
export const SMALL_WINDOW_MS = 100;
export const SMALL_WINDOW_RUNS = 9;
export const SMALL_WINDOW_FLOOR_MS = 1;
export const SMALL_WINDOW_FLOOR_RATIO = 0.05;
const SMALL_WINDOW_FIELDS = ['cpuMs', 'steadyCpuMs'];

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

/** Total public pairs once the first `runs` pairs' TS samples are known. */
export function smallWindowRunCount(runs, tsSamples) {
  const small = SMALL_WINDOW_FIELDS.some((field) => median(tsSamples.map((sample) => sample[field])) < SMALL_WINDOW_MS);
  return small ? Math.max(runs, SMALL_WINDOW_RUNS) : runs;
}

/** How far Rust may exceed a small-window TS median. */
export const smallWindowAllowanceMs = (reference) => Math.max(SMALL_WINDOW_FLOOR_MS, reference * SMALL_WINDOW_FLOOR_RATIO);

export function performanceMetricErrors(metrics, mode) {
  const errors = [];
  if (metrics?.baselineKind !== 'idle-loaded') errors.push('missing idle-loaded memory baseline');
  if (metrics?.measuredMode !== mode) errors.push(`invalid measured mode (expected ${mode})`);
  for (const field of ['cpuMs', 'steadyCpuMs', 'processCpuMs']) {
    if (!Number.isFinite(metrics?.[field]) || metrics[field] <= 0) errors.push(`invalid ${field} (${metrics?.[field]})`);
  }
  return errors;
}

export const CPU_FIELDS = Object.freeze(['cpuMs', 'steadyCpuMs', 'processCpuMs']);
const OBJECTIVES = Object.freeze([
  ['cpuMs', 'main-thread CPU'], ['steadyCpuMs', 'steady-state CPU'],
  ['processCpuMs', 'process CPU'], ['retainedBytes', 'retained memory'],
]);

/** The original bound, also used by the separate two-run certifier. */
export function performanceObjectives(ts, rust) {
  return OBJECTIVES.map(([field, label]) => {
    const reference = median(ts.map((sample) => sample[field]));
    const candidate = median(rust.map((sample) => sample[field]));
    const category = SMALL_WINDOW_FIELDS.includes(field) && reference < SMALL_WINDOW_MS ? 'small' : 'relative';
    const limit = category === 'small' ? reference + smallWindowAllowanceMs(reference) : reference * (1 - REQUIRED_IMPROVEMENT);
    return { field, label, reference, candidate, category, limit, ratio: candidate / limit };
  });
}

export function performanceFailures(ts, rust) {
  const errors = [];
  for (const { label, reference, candidate, category, limit } of performanceObjectives(ts, rust)) {
    if (!Number.isFinite(reference) || reference <= 0 || !Number.isFinite(candidate))
      errors.push(`performance ${label}: no valid positive TS baseline`);
    else if (category === 'small') {
      if (!(candidate <= limit))
        errors.push(`performance ${label}: Rust must be within max(1 ms, 5%) of a sub-${SMALL_WINDOW_MS} ms TS objective (ts=${reference}, rust=${candidate})`);
    } else if (!(candidate <= limit))
      errors.push(`performance ${label}: Rust must be at least 10% below TS (ts=${reference}, rust=${candidate})`);
  }
  return errors;
}
