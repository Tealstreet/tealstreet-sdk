import { GATE_SUITES } from './ci-gate-suites.mjs';

// Sam's shared policy applies to every registered CPU-gated suite.
export const CPU_REMEASURE_SUITES = Object.freeze([...GATE_SUITES]);
export const cpuRemeasureEnabled = (suite) => CPU_REMEASURE_SUITES.includes(suite);
export function requireCpuRemeasureSuite(suite) {
  if (!cpuRemeasureEnabled(suite)) throw new Error(`CPU remeasurement is not enabled for suite ${JSON.stringify(suite)}`);
  return suite;
}
