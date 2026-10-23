// One shared policy, initially enabled only for the approved suite.
// Adding a suite requires Sam's explicit decision, not a venue-owned override.
export const CPU_REMEASURE_SUITES = Object.freeze(['bitget']);
export const cpuRemeasureEnabled = (suite) => CPU_REMEASURE_SUITES.includes(suite);
export function requireCpuRemeasureSuite(suite) {
  if (!cpuRemeasureEnabled(suite)) throw new Error(`CPU remeasurement is not enabled for suite ${JSON.stringify(suite)}`);
  return suite;
}
