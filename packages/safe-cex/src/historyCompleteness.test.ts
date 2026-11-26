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

it('carries the closed reason code across clone and JSON, non-enumerable on the restored rows', () => {
  const rows = Object.defineProperty(Object.defineProperty([], 'incomplete', { value: true }), 'incompleteReason', {
    value: 'pageFull',
  });
  const wire = JSON.parse(JSON.stringify({ rows, completeness: historyCompletenessOf(rows) }));
  expect(wire.completeness).toEqual({ incomplete: true, reason: 'pageFull' });
  const restored = withHistoryCompleteness(wire.rows, wire.completeness);
  expect(restored).toHaveProperty('incompleteReason', 'pageFull');
  expect(Object.getOwnPropertyDescriptor(restored, 'incompleteReason')?.enumerable).toBe(false);
  expect(historyCompletenessOf(copyHistoryCompleteness(rows, ['mapped']))).toEqual({
    incomplete: true,
    reason: 'pageFull',
  });
});

it('drops a reason that is not a short identifier, keeping the incompleteness', () => {
  for (const reason of ['venue said: ip 1.2.3.4', '', 42, 'x'.repeat(41)]) {
    const rows = Object.defineProperty(Object.defineProperty([], 'incomplete', { value: true }), 'incompleteReason', {
      value: reason,
    });
    expect(historyCompletenessOf(rows)).toEqual({ incomplete: true });
    expect(withHistoryCompleteness([], { incomplete: true, reason })).not.toHaveProperty('incompleteReason');
  }
});

it('keeps a later-page failure through mapping and JSON without raw native values', () => {
  const failure = {
    operation: 'bybitOrders',
    kind: 'rateLimited',
    venueCode: '10006',
    message: 'PRIVATE_CANARY',
  };
  const input = Object.defineProperties([{ id: 1 }], {
    incomplete: { value: true },
    incompleteReason: { value: 'laterPageFailed' },
    historyFailure: { value: failure },
  });
  const mapped = copyHistoryCompleteness(
    input,
    input.map((row) => ({ ...row })),
  );
  const wire = JSON.parse(JSON.stringify({ rows: mapped, completeness: historyCompletenessOf(mapped) }));
  expect(wire).toEqual({
    rows: [{ id: 1 }],
    completeness: {
      incomplete: true,
      reason: 'laterPageFailed',
      failure: { operation: 'bybitOrders', kind: 'rateLimited', venueCode: '10006' },
    },
  });
  const restored = withHistoryCompleteness(wire.rows, wire.completeness);
  expect(historyCompletenessOf(restored)).toEqual(wire.completeness);
  expect(Object.getOwnPropertyDescriptor(restored, 'historyFailure')?.enumerable).toBe(false);
  expect(JSON.stringify(wire)).not.toContain('PRIVATE_CANARY');
});

it('does not carry a different input failure when replacing a completeness assertion', () => {
  const rows = withHistoryCompleteness([], {
    incomplete: true,
    reason: 'laterPageFailed',
    failure: { operation: 'bybitClosedPnl', kind: 'network' },
  });
  const updated = withHistoryCompleteness(rows, { incomplete: true, reason: 'pageBudget' });
  expect(updated).not.toHaveProperty('historyFailure');
  const frozen = Object.freeze(
    withHistoryCompleteness([], {
      incomplete: true,
      reason: 'laterPageFailed',
      failure: { operation: 'bybitOrders', kind: 'timeout' },
    }),
  );
  expect(withHistoryCompleteness(frozen, { incomplete: true, reason: 'cursorStalled' })).not.toHaveProperty(
    'historyFailure',
  );
});

it('drops malformed failure metadata and never attaches failure facts without the owning reason', () => {
  for (const failure of [
    null,
    [],
    { operation: 'PRIVATE_CANARY', kind: 'network' },
    { operation: 'bybitOrders', kind: 'PRIVATE_CANARY' },
    { operation: 'bybitOrders', kind: 'network', venueCode: 'PRIVATE_CANARY' },
    {
      get operation() {
        throw Error('PRIVATE_CANARY');
      },
    },
  ]) {
    expect(withHistoryCompleteness([], { incomplete: true, reason: 'laterPageFailed', failure })).not.toHaveProperty(
      'historyFailure',
    );
  }
  expect(
    withHistoryCompleteness([], {
      incomplete: true,
      reason: 'pageBudget',
      failure: { operation: 'bybitOrders', kind: 'network' },
    }),
  ).not.toHaveProperty('historyFailure');
});
