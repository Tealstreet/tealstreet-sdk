import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpuObjectiveMet, pairsFor, SMALL_WINDOW_PAIRS } from './cpu-objective.mjs';

test('objectives of 100 ms or more stay strict', () => {
  assert.equal(cpuObjectiveMet(99.9, 100), true);
  assert.equal(cpuObjectiveMet(100, 100), false);
  assert.equal(cpuObjectiveMet(323.3, 310.6), false);
});

test('objectives under 100 ms allow Rust max(1 ms, 5%) above TS', () => {
  // Under 20 ms the 1 ms floor dominates (landing Hyro steady misses).
  for (const [ts, rust] of [[5.6, 5.9], [5.3, 5.7], [5.7, 6.3], [11.3, 12.2]]) assert.equal(cpuObjectiveMet(rust, ts), true);
  assert.equal(cpuObjectiveMet(6.5, 5.5), true);
  assert.equal(cpuObjectiveMet(6.51, 5.5), false);
  // Above 20 ms the 5% allowance dominates.
  assert.equal(cpuObjectiveMet(42.0, 40.0), true);
  assert.equal(cpuObjectiveMet(42.1, 40.0), false);
  assert.equal(cpuObjectiveMet(103.9, 99.0), true);
  assert.equal(cpuObjectiveMet(104.0, 99.0), false);
});

test('missing or invalid measurements never pass', () => {
  for (const [rust, ts] of [[NaN, 5], [5, NaN], [undefined, 5], [Infinity, 5], [1, Infinity]])
    assert.equal(cpuObjectiveMet(rust, ts), false);
});

test('objectives under 100 ms get more pairs; larger ones keep the requested count', () => {
  assert.equal(pairsFor(3, [165, 18.6]), SMALL_WINDOW_PAIRS);
  assert.equal(pairsFor(3, [45, 150]), SMALL_WINDOW_PAIRS);
  assert.equal(pairsFor(3, [165, 100]), 3);
  assert.equal(pairsFor(11, [5, 5]), 11);
  assert.equal(pairsFor(1, [NaN, NaN]), 1);
});
