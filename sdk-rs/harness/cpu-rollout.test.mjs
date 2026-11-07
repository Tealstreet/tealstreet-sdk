import assert from 'node:assert/strict';
import { test } from 'node:test';
import { GATE_SUITES } from './ci-gate-suites.mjs';
import { CPU_REMEASURE_SUITES, cpuRemeasureEnabled, requireCpuRemeasureSuite } from './cpu-rollout.mjs';

test('the shared policy enables every exact CPU-gated suite and rejects unknown aliases', () => {
  assert.deepEqual(CPU_REMEASURE_SUITES, GATE_SUITES);
  assert.equal(Object.isFrozen(CPU_REMEASURE_SUITES), true);
  for (const suite of GATE_SUITES) {
    assert.equal(cpuRemeasureEnabled(suite), true);
    assert.equal(requireCpuRemeasureSuite(suite), suite);
  }
  for (const suite of ['unknown', 'bitgetinverse', '', undefined]) {
    assert.equal(cpuRemeasureEnabled(suite), false);
    assert.throws(() => requireCpuRemeasureSuite(suite), /not enabled/);
  }
});
