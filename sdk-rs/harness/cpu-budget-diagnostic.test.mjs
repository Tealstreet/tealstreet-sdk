import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AA_ORDER, budgetComparison } from './cpu-budget-diagnostic.mjs';

test('A/A uses every fixed block; twelve and sixteen are declared diagnostic budgets', () => {
  assert.deepEqual(AA_ORDER, ['A', 'B', 'B', 'A', 'A', 'B', 'B', 'A']);
  const samples = Object.fromEntries(['A', 'B'].map((arm) => [arm, Object.fromEntries(['ts', 'rust'].map((impl) => [impl,
    Array.from({ length: 16 }, () => ({ steadyCpuMs: impl === 'ts' ? 18 : 20 }))]))]));
  assert.equal(budgetComparison(samples, 12).relativeRatio, 1);
  samples.B.rust[15].steadyCpuMs = 200;
  assert.equal(samples.B.rust.length, 16);
  assert.equal(budgetComparison(samples, 16).stableWithinFivePercent, true);
  assert.throws(() => budgetComparison(samples, 9));
  samples.B.rust.pop();
  assert.throws(() => budgetComparison(samples, 12));
});
