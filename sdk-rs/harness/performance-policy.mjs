// The replay gate's timing policy, in one place.
//
// Sam, 2026-11-07: Rust must not be slower than TS. A CPU objective (main-thread,
// steady-state, process) fails only when the Rust median exceeds 1.00x the TS median
// after one in-job re-measure; missing the 10%-faster target is a warning.
//
// Public tapes run a fixed even budget of interleaved TS/Rust pairs, alternating which
// side runs first, and compare per-metric medians. The 10%-faster target below is
// still computed for CPU (as a warning) and still binds retained memory.
//
// Small windows (main-thread/steady CPU with a TS median under 100 ms) keep their
// max(1 ms, 5%) limit only for the remeasure objectives and draw no 10%-target
// warning; like every CPU objective they fail above 1.00x after the re-measure.

export const PUBLIC_REPLAY_PAIRS = 16;
export const REQUIRED_IMPROVEMENT = 0.10;
export const SMALL_WINDOW_MS = 100;
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
  const count = Number(raw ?? (isPrivate ? 1 : PUBLIC_REPLAY_PAIRS));
  if (!Number.isSafeInteger(count) || count < (isPrivate ? 1 : PUBLIC_REPLAY_PAIRS) || (!isPrivate && count % 2 !== 0))
    throw new Error(isPrivate ? 'private runs must be a positive integer' : `public performance requires at least ${PUBLIC_REPLAY_PAIRS} balanced interleaved runs`);
  return count;
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

/** Rust/TS above this, on the same runner's paired medians, is a CPU failure. */
export const CPU_FAIL_RATIO = 1.00;
export const CPU_SLOWER = 'Rust is slower than TS';
const ratioText = (reference, candidate) => `${(candidate / reference).toFixed(3)}x`;

/** Blocking failures and non-blocking CPU warnings for one measured sample set. */
export function performanceVerdict(ts, rust) {
  const failures = [], warnings = [];
  for (const { field, label, reference, candidate, category, limit } of performanceObjectives(ts, rust)) {
    if (!Number.isFinite(reference) || reference <= 0 || !Number.isFinite(candidate))
      failures.push(`performance ${label}: no valid positive TS baseline`);
    else if (!CPU_FIELDS.includes(field)) {
      if (!(candidate <= limit))
        failures.push(`performance ${label}: Rust must be at least 10% below TS (ts=${reference}, rust=${candidate})`);
    } else if (!(candidate <= reference * CPU_FAIL_RATIO))
      failures.push(`performance ${label}: ${CPU_SLOWER} (ts=${reference}, rust=${candidate}, ${ratioText(reference, candidate)})`);
    else if (category === 'relative' && !(candidate <= limit))
      warnings.push(`performance ${label}: missed the 10%-faster target (ts=${reference}, rust=${candidate}, ${ratioText(reference, candidate)})`);
  }
  return { failures, warnings };
}

export const performanceFailures = (ts, rust) => performanceVerdict(ts, rust).failures;

const isSlower = (failure) => failure.includes(`: ${CPU_SLOWER} (`);
/** True when a CPU objective is over the 1.00x line, the one case that is re-measured. */
export const cpuSlower = (failures) => failures.some(isSlower);

/** The reported verdict after at most one in-job re-measure: the re-measured set decides,
 * and a first miss that cleared on it is kept as a warning. */
export function remeasuredVerdict(first, remeasured) {
  if (!remeasured) return first;
  const cleared = cpuSlower(remeasured.failures) ? [] : first.failures.filter(isSlower).map((failure) => `${failure}; cleared on the one re-measure`);
  return { failures: remeasured.failures, warnings: [...remeasured.warnings, ...cleared].sort() };
}
