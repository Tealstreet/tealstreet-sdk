import { describe, expect, it } from 'vitest';

import { oldestCursorId } from '../../utils/oldestCursorId';

/**
 * The third occurrence of "read a position out of a sorted list without establishing the
 * sort direction" was the bitget idleness verdict; the first two were the weex pager and the
 * income pager's uncovered-side inference. okx and blofin still derived their paging cursor
 * from `rows[rows.length - 1]`, which is correct only while those venues return newest-first.
 */

describe('oldestCursorId', () => {
  it('returns the same id whichever ORDER the page arrives in', () => {
    // The whole point: a cursor derived from array position silently walks the wrong way if
    // the venue flips its sort, and the walk then ends one page in.
    const descending = [{ billId: '900' }, { billId: '800' }, { billId: '700' }];
    const ascending = [...descending].reverse();

    expect(oldestCursorId(descending, 'billId')).toBe('700');
    expect(oldestCursorId(ascending, 'billId')).toBe('700');
    // The defect being replaced: last-element would give 700 on one and 900 on the other.
    expect(descending[descending.length - 1].billId).not.toBe(ascending[ascending.length - 1].billId);
  });

  it('does NOT parse ids to Number — okx billIds exceed MAX_SAFE_INTEGER', () => {
    // These two differ only in the final digit and both round to the same float. parseInt
    // would consider them equal and could pick the wrong cursor, stalling the walk.
    const a = '1439829381856505906';
    const b = '1439829381856505907';
    expect(Number(a)).toBe(Number(b));
    expect(oldestCursorId([{ id: b }, { id: a }], 'id')).toBe(a);
  });

  it('orders by magnitude, not lexicographically', () => {
    // '1000' < '999' as strings. Length-first is what makes the compare correct.
    expect(oldestCursorId([{ id: '1000' }, { id: '999' }], 'id')).toBe('999');
  });

  it('skips non-numeric ids rather than ordering them arbitrarily', () => {
    expect(oldestCursorId([{ id: 'abc' }, { id: '500' }], 'id')).toBe('500');
  });

  it('returns undefined when nothing is usable, so the caller stops instead of looping', () => {
    expect(oldestCursorId([], 'id')).toBeUndefined();
    expect(oldestCursorId([{ id: '' }, { id: null }, { other: '1' }], 'id')).toBeUndefined();
  });
});
