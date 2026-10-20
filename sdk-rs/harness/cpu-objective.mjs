// Rust must use less CPU than TS. Objectives under 10 ms TS are a few ms of
// mostly shared replay work, where tied implementations differ by up to 0.9 ms
// run to run (docs/plans/RUST_GATE_TIMING.md); there Rust may exceed TS by 1 ms.
export const SMALL_OBJECTIVE_MS = 10;
export const SMALL_OBJECTIVE_TOLERANCE_MS = 1;

export const cpuObjectiveMet = (rustMs, tsMs) =>
  Number.isFinite(rustMs) &&
  Number.isFinite(tsMs) &&
  (tsMs < SMALL_OBJECTIVE_MS ? rustMs <= tsMs + SMALL_OBJECTIVE_TOLERANCE_MS : rustMs < tsMs);
