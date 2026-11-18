import assert from 'node:assert/strict';
import test from 'node:test';
import { assertComparisonParents } from './compare-wasm-size.mjs';

test('comparison rejects a branch head, a drifting merge, and a swapped master parent', () => {
  const merge = 'a'.repeat(40), base = 'b'.repeat(40), head = 'c'.repeat(40);
  const selection = { merge, parents: [base, head], expectedMerge: merge, expectedBase: base, expectedHead: head };
  assert.doesNotThrow(() => assertComparisonParents(selection));
  for (const patch of [
    { parents: [base] }, { merge: 'd'.repeat(40) }, { parents: [head, base] },
    { parents: [base, 'd'.repeat(40)] }, { expectedBase: 'master' },
  ]) assert.throws(() => assertComparisonParents({ ...selection, ...patch }));
});
