/**
 * The SMALLEST id in a page, for venues whose cursor means "older than this id".
 *
 * Replaces `rows[rows.length - 1].id`, which is the same defect this epic has now hit three
 * times in different clothes: **reading a position out of a sorted list without establishing
 * the sort direction.** The bitget idleness verdict read the first page of an ascending list
 * and called the account idle; the weex pager walked a backwards cursor against an
 * ascending venue and stopped after one page; the income pager inferred which end of a
 * window had been served.
 *
 * Taking the last element is correct only while the venue returns DESCENDING. okx and blofin
 * both do today, so this was correct by accident — and the failure mode if either ever flips
 * is silent: the cursor walks the wrong way, the second page returns nothing, and the walk
 * terminates one page in. On a venue that currently PASSES the canonical gate that would
 * corrupt a green verdict rather than produce an obviously empty one.
 *
 * **Ids are compared as unsigned integers WITHOUT parsing to Number.** okx billIds and
 * blofin tradeIds routinely exceed `Number.MAX_SAFE_INTEGER`, so `parseInt` would collapse
 * distinct ids onto the same value and pick the wrong cursor. Length-then-lexicographic is
 * exact for non-negative integer strings and needs no BigInt.
 */

/** Numeric-string compare: shorter is smaller, equal length falls back to lexicographic. */
const isSmaller = (candidate: string, current: string): boolean => {
  if (candidate.length !== current.length) return candidate.length < current.length;
  return candidate < current;
};

export const oldestCursorId = (rows: Array<Record<string, any>>, field: string): string | undefined => {
  let oldest: string | undefined;

  for (const row of rows) {
    const raw = row?.[field];
    if (raw === undefined || raw === null) continue;
    const id = String(raw).trim();
    // A non-numeric id cannot be ordered this way; skipping it is safer than guessing, and
    // an all-skipped page simply yields `undefined` so the caller stops rather than looping.
    if (!id || !/^\d+$/.test(id)) continue;
    if (oldest === undefined || isSmaller(id, oldest)) oldest = id;
  }

  return oldest;
};
