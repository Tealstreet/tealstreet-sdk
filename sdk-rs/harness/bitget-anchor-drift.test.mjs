import test from 'node:test';
import assert from 'node:assert/strict';
import { DRIFT_ORDER, assessAnchorDrift } from './bitget-anchor-drift.mjs';

const rows = (ratio = 1) => DRIFT_ORDER.map((arm) => ({ arm,
  samples: Array.from({ length: 16 }, () => arm === 'old' ? 10 : 10 * ratio) }));

test('fixed counterbalanced Rust-only comparison records drift without certifying', () => {
  const result = assessAnchorDrift(rows(1.02));
  assert.equal(result.oldMedianMs, 10);
  assert.equal(result.newMedianMs, 10.2);
  assert.equal(result.ratio, 1.02);
  assert.equal(result.withinTolerance, true);
  assert.equal(result.certified, false);
  assert.equal(result.correctness, 'known-changed');
});

test('drift above the standing tolerance blocks reset', () => {
  assert.equal(assessAnchorDrift(rows(1.024)).withinTolerance, false);
});

test('missing, reordered and invalid CPU evidence never permits reset', () => {
  for (const mutate of [x => x.pop(), x => { x[0].arm = 'new'; },
    x => x[2].samples.pop(), x => { x[0].samples[0] = NaN; },
    x => { x[0].samples[0] = 0; }]) {
    const data = rows(); mutate(data);
    assert.throws(() => assessAnchorDrift(data));
  }
});
