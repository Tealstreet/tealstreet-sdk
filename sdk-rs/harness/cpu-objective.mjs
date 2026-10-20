// Rust must use less CPU than TS. Objectives under 10 ms TS are a few ms of
// mostly shared replay work, where tied implementations differ by up to 0.9 ms
// run to run (docs/plans/RUST_GATE_TIMING.md); there Rust may exceed TS by 1 ms.
export const SMALL_OBJECTIVE_MS = 10;
export const SMALL_OBJECTIVE_TOLERANCE_MS = 1;

export const cpuObjectiveMet = (rustMs, tsMs) =>
  Number.isFinite(rustMs) &&
  Number.isFinite(tsMs) &&
  (tsMs < SMALL_OBJECTIVE_MS ? rustMs <= tsMs + SMALL_OBJECTIVE_TOLERANCE_MS : rustMs < tsMs);

// A CPU objective whose TS median is under 20 ms after the first pairs gets
// more interleaved pairs: a 50-frame steady window moves ~10% run to run,
// which three samples cannot resolve (docs/plans/RUST_GATE_TIMING.md).
export const SMALL_WINDOW_MS = 20;
export const SMALL_WINDOW_PAIRS = 9;

/** Total pairs to run given the TS medians observed so far. */
export const pairsFor = (initialPairs, tsMedians) =>
  tsMedians.some((ms) => Number.isFinite(ms) && ms < SMALL_WINDOW_MS)
    ? Math.max(initialPairs, SMALL_WINDOW_PAIRS)
    : initialPairs;
