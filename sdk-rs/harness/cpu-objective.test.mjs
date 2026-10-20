import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpuObjectiveMet } from './cpu-objective.mjs';

test('objectives of 10 ms or more stay strict', () => {
  assert.equal(cpuObjectiveMet(10.9, 11), true);
  assert.equal(cpuObjectiveMet(11, 11), false);
  assert.equal(cpuObjectiveMet(10.1, 10), false);
  assert.equal(cpuObjectiveMet(323.3, 310.6), false);
});

test('objectives under 10 ms allow Rust at most 1 ms above TS', () => {
  // Landing CI Hyro steady misses (TS/Rust ms): runs 39441654489, 39438422053, 39433770908.
  for (const [ts, rust] of [[5.6, 5.9], [5.3, 5.7], [5.7, 6.3]]) assert.equal(cpuObjectiveMet(rust, ts), true);
  assert.equal(cpuObjectiveMet(6.5, 5.5), true);
  assert.equal(cpuObjectiveMet(6.51, 5.5), false);
  assert.equal(cpuObjectiveMet(11, 9.9), false);
});

test('missing or invalid measurements never pass', () => {
  for (const [rust, ts] of [[NaN, 5], [5, NaN], [undefined, 5], [Infinity, 5], [1, Infinity]])
    assert.equal(cpuObjectiveMet(rust, ts), false);
});
