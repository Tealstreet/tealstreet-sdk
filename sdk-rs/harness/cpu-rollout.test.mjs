import assert from 'node:assert/strict';
import { test } from 'node:test';
import { GATE_SUITES } from './ci-gate-suites.mjs';
import { CPU_REMEASURE_SUITES, cpuRemeasureEnabled, requireCpuRemeasureSuite } from './cpu-rollout.mjs';

test('the shared policy is enabled for Bitget only, with no implicit family/alias rollout', () => {
  assert.deepEqual(CPU_REMEASURE_SUITES, ['bitget']);
  assert.equal(Object.isFrozen(CPU_REMEASURE_SUITES), true);
  assert.equal(requireCpuRemeasureSuite('bitget'), 'bitget');
  for (const suite of [...GATE_SUITES.filter((name) => name !== 'bitget'), 'bitgetinverse', '', undefined]) {
    assert.equal(cpuRemeasureEnabled(suite), false);
    assert.throws(() => requireCpuRemeasureSuite(suite), /not enabled/);
  }
});
