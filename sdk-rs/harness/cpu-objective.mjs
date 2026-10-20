// Rust must use less CPU than TS. Windows under 100 ms TS are tens of ms of
// mostly shared replay work, where tied implementations move several percent
// run to run (docs/plans/RUST_GATE_TIMING.md). There the gate runs more
// interleaved pairs and allows Rust max(1 ms, 5%) above TS; larger windows
// stay strict.
export const SMALL_WINDOW_MS = 100;
export const SMALL_WINDOW_PAIRS = 9;
export const SMALL_WINDOW_FLOOR_MS = 1;
export const SMALL_WINDOW_FLOOR_RATIO = 0.05;

/** The most Rust may exceed TS on an objective with this TS median. */
export const allowanceMs = (tsMs) =>
  tsMs < SMALL_WINDOW_MS ? Math.max(SMALL_WINDOW_FLOOR_MS, tsMs * SMALL_WINDOW_FLOOR_RATIO) : 0;

export const cpuObjectiveMet = (rustMs, tsMs) =>
  Number.isFinite(rustMs) &&
  Number.isFinite(tsMs) &&
  (tsMs < SMALL_WINDOW_MS ? rustMs <= tsMs + allowanceMs(tsMs) : rustMs < tsMs);

/** Total pairs to run given the TS medians observed so far. */
export const pairsFor = (initialPairs, tsMedians) =>
  tsMedians.some((ms) => Number.isFinite(ms) && ms < SMALL_WINDOW_MS)
    ? Math.max(initialPairs, SMALL_WINDOW_PAIRS)
    : initialPairs;
