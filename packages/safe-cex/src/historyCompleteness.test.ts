import { expect, it } from 'vitest';

import { copyHistoryCompleteness, historyCompletenessOf, withHistoryCompleteness } from './historyCompleteness';

it('carries a serializable positive record across clone and JSON when the array marker disappears', () => {
  const rows = Object.defineProperty([], 'incomplete', { value: true });
  const wire = JSON.parse(JSON.stringify({ rows, completeness: historyCompletenessOf(rows) }));
  expect(wire.rows).not.toHaveProperty('incomplete');
  expect(wire.completeness).toEqual({ incomplete: true });
  const restored = withHistoryCompleteness(wire.rows, wire.completeness);
  expect(restored).toHaveProperty('incomplete', true);
  expect(Object.getOwnPropertyDescriptor(restored, 'incomplete')?.enumerable).toBe(false);
});

it.each([
  undefined,
  null,
  false,
  true,
  'incomplete',
  {},
  { incomplete: false },
  { incomplete: 'true' },
  Object.create({ incomplete: true }),
])('does not infer completeness from malformed or absent metadata %j', (metadata) => {
  const rows: unknown[] = [];
  expect(withHistoryCompleteness(rows, metadata)).toBe(rows);
  expect(historyCompletenessOf(rows)).toBeUndefined();
});

it('does not attach array metadata to ordinary object results or inherit a prototype marker', () => {
  const object = { value: 1 };
  expect(withHistoryCompleteness(object, { incomplete: true })).toBe(object);
  expect(historyCompletenessOf({ incomplete: true })).toBeUndefined();
  const rows: unknown[] = [];
  Object.setPrototypeOf(rows, Object.create(Array.prototype, { incomplete: { value: true } }));
  expect(historyCompletenessOf(rows)).toBeUndefined();
});

it('preserves incompleteness after filtering every row without modifying the input', () => {
  const input = Object.defineProperty([{ id: 1 }], 'incomplete', { value: true });
  const result = copyHistoryCompleteness(
    input,
    input.filter(() => false),
  );
  expect(result).toEqual([]);
  expect(result).toHaveProperty('incomplete', true);
  expect(input).toEqual([{ id: 1 }]);
});

it('restores a non-enumerable marker even on frozen or premarked arrays', () => {
  const frozen = Object.freeze([1]);
  const restored = withHistoryCompleteness(frozen, { incomplete: true });
  expect(restored).toEqual([1]);
  expect(restored).toHaveProperty('incomplete', true);
  expect(frozen).not.toHaveProperty('incomplete');
  const enumerable = Object.assign([], { incomplete: true });
  expect(
    Object.getOwnPropertyDescriptor(withHistoryCompleteness(enumerable, { incomplete: true }), 'incomplete')
      ?.enumerable,
  ).toBe(false);
});
