import type { IncomeRecord } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { paginateIncomeRange } from '../../exchanges/income-paging';

/**
 * Fixtures for the SHARED income pager (weex + binance).
 *
 * weex has NO credential in the `yarn keys` sheet, so these are the only thing standing
 * between it and shipping unexercised. Binance does have one and is verified live as well —
 * but binance is also the venue that currently PASSES the canonical gate, so a silent
 * truncation there corrupts a green verdict. Both defects are encoded below: weex issued one
 * request per chunk with no cursor; binance gated both ends of its loop on a full-page count.
 */

const DAY = 24 * 60 * 60 * 1000;
const HUNDRED_DAYS = 100 * DAY;

const record = (timestamp: number, id: string): IncomeRecord => ({
  symbol: 'BTCUSDT',
  incomeType: 'REALIZED_PNL',
  income: 1,
  asset: 'USDT',
  timestamp,
  tranId: id,
  category: 'realized_pnl',
});

/**
 * A venue holding `total` records spaced a minute apart, answering at most `limit` per call.
 *
 * `order` is the knob that matters: WEEX's sort order is unverified, so the pager must behave
 * identically either way. A pager that assumed newest-first would walk the cursor the wrong
 * direction against an oldest-first venue and stop after one page — the original bug wearing
 * a paging costume.
 */
const fakeVenue = (opts: { total: number; newest: number; order: 'newest-first' | 'oldest-first'; cap?: number }) => {
  const all = Array.from({ length: opts.total }, (_, i) => record(opts.newest - i * 60_000, `id-${i}`));
  const calls: Array<{ startTime: number; endTime: number; limit: number }> = [];

  const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) => {
    calls.push({ startTime, endTime, limit });
    const inWindow = all.filter((r) => r.timestamp >= startTime && r.timestamp <= endTime);
    const sorted =
      opts.order === 'newest-first'
        ? [...inWindow].sort((a, b) => b.timestamp - a.timestamp)
        : [...inWindow].sort((a, b) => a.timestamp - b.timestamp);
    return sorted.slice(0, Math.min(limit, opts.cap ?? limit));
  });

  return { all, calls, fetchPage };
};

describe('paginateIncomeRange', () => {
  it('THE BUG: returns every record, not just the first page of each chunk', async () => {
    // 250 records inside ONE 100-day chunk. The old implementation issued a single request
    // and returned 100 of them, losing 60% of the ledger without a word.
    const newest = 1_700_000_000_000;
    const venue = fakeVenue({ total: 250, newest, order: 'newest-first' });

    const result = await paginateIncomeRange(venue.fetchPage, {
      startTime: newest - 90 * DAY,
      endTime: newest + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(250);
    expect(result.truncated).toBe(false);
    // Proof it actually paged rather than getting lucky with a big page.
    expect(venue.fetchPage.mock.calls.length).toBeGreaterThan(1);
  });

  it('behaves identically when the venue returns OLDEST-first', async () => {
    // The sort order is unverified against the live venue, so it must not be load-bearing.
    const newest = 1_700_000_000_000;
    const venue = fakeVenue({ total: 250, newest, order: 'oldest-first' });

    const result = await paginateIncomeRange(venue.fetchPage, {
      startTime: newest - 90 * DAY,
      endTime: newest + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(250);
    expect(result.truncated).toBe(false);
  });

  it('does NOT stop on a short page — truncation is a span question, not a row count', async () => {
    // The venue caps at 40 rows despite being asked for 100. Every "did I get a full page?"
    // pager stops here and silently drops the rest; this one keeps going because the records
    // have not yet reached the start of the window.
    const newest = 1_700_000_000_000;
    const venue = fakeVenue({ total: 200, newest, order: 'newest-first', cap: 40 });

    const result = await paginateIncomeRange(venue.fetchPage, {
      startTime: newest - 90 * DAY,
      endTime: newest + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(200);
    expect(result.truncated).toBe(false);
  });

  it('spans multiple chunks when the range exceeds the venue window', async () => {
    const newest = 1_700_000_000_000;
    // 400 records a day apart = ~400 days, four 100-day chunks.
    const all = Array.from({ length: 400 }, (_, i) => record(newest - i * DAY, `id-${i}`));
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) =>
      all
        .filter((r) => r.timestamp >= startTime && r.timestamp <= endTime)
        .sort((a, b) => b.timestamp - a.timestamp)
        .slice(0, limit),
    );

    const result = await paginateIncomeRange(fetchPage, {
      startTime: newest - 399 * DAY,
      endTime: newest + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(400);
    expect(result.truncated).toBe(false);
  });

  it('deduplicates rows returned twice across branch boundaries', async () => {
    // Branches meet at their edges by construction, so the same row is legitimately served
    // more than once. Without identity dedupe the ledger would double-count it.
    const t = 1_700_000_000_000;
    const all = [record(t, 'a'), record(t - 1000, 'b'), record(t - 2000, 'c')];
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) =>
      all
        .filter((r) => r.timestamp >= startTime && r.timestamp <= endTime)
        .sort((a, b) => b.timestamp - a.timestamp)
        .slice(0, limit),
    );

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 10_000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 2,
    });

    expect(result.records.map((r) => r.tranId).sort()).toEqual(['a', 'b', 'c']);
    expect(result.truncated).toBe(false);
  });

  it('REPORTS the one case a time cursor genuinely cannot walk, and still gets the rest', async () => {
    // Three records share a single millisecond with a page limit of 2. No time-based cursor
    // can reach the third — this endpoint has no offset or id cursor to fall back on. The
    // invariant is that this is ANNOUNCED rather than returned as a short ledger, and that
    // one stuck millisecond does not cost the rest of the window.
    const t = 1_700_000_000_000;
    const all = [record(t, 'a'), record(t, 'b'), record(t, 'c'), record(t - 1000, 'd')];
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) =>
      all
        .filter((r) => r.timestamp >= startTime && r.timestamp <= endTime)
        .sort((a, b) => b.timestamp - a.timestamp)
        .slice(0, limit),
    );

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 10_000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 2,
    });

    expect(result.truncated).toBe(true);
    // `d` sits past the saturated instant and is still recovered.
    expect(result.records.map((r) => r.tranId)).toContain('d');
  });

  it('REPORTS truncation rather than returning a quietly partial ledger', async () => {
    // A venue that ignores the window entirely: every page answers with the same newest rows
    // regardless of what was asked. No branch can be proven empty, so the walk cannot claim
    // completeness — the point is that it says so, because a partial ledger reconciles fine.
    const t = 1_700_000_000_000;
    const all = Array.from({ length: 500 }, (_, i) => record(t - i * 1000, `id-${i}`));
    const fetchPage = vi.fn(async ({ limit }: any) => all.slice(0, limit));

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 400 * 1000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 10,
      maxPagesPerChunk: 5,
    });

    expect(result.truncated).toBe(true);
    expect(result.exhaustedChunks).toBe(1);
    // And it does not spin: the budget is a backstop, not the termination condition.
    expect(fetchPage.mock.calls.length).toBeLessThanOrEqual(5);
  });

  it('terminates on an empty page without reporting truncation', async () => {
    const t = 1_700_000_000_000;
    const fetchPage = vi.fn(async () => []);

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 10 * DAY,
      endTime: t,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toEqual([]);
    // An idle account is not a truncated one, and the difference is the whole point.
    expect(result.truncated).toBe(false);
    expect(fetchPage).toHaveBeenCalledTimes(1);
  });

  it('reports truncation when timestamps are unusable instead of dropping the chunk', async () => {
    const t = 1_700_000_000_000;
    const fetchPage = vi.fn(async () => [{ ...record(t, 'x'), timestamp: Number.NaN }]);

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 10 * DAY,
      endTime: t,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.truncated).toBe(true);
  });

  it('keeps rows distinct when the venue omits tranId', async () => {
    // `normalizeIncome` runs `toIntegerString(billId ?? tranId ?? id)`, which yields '' when
    // the venue sends none of them — so this is the shape that actually reaches the pager.
    // Keying on that shared empty id would collapse every untagged row into one.
    const t = 1_700_000_000_000;
    const all: IncomeRecord[] = [
      { ...record(t, ''), income: 1 },
      { ...record(t, ''), income: 2 },
    ];
    const fetchPage = vi.fn(async ({ endTime }: any) => (endTime >= t ? all : []));

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - DAY,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(2);
  });

  it('rejects a nonsensical range without issuing a request', async () => {
    const fetchPage = vi.fn(async () => []);
    const result = await paginateIncomeRange(fetchPage, {
      startTime: 100,
      endTime: 100,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toEqual([]);
    expect(fetchPage).not.toHaveBeenCalled();
  });

  it('reads the ORDER off the page and skips the branch that cannot hold data', async () => {
    // An ascending page holds the OLDEST rows of the window, so only the part above `newest`
    // can still contain anything. Reading that from the response — rather than assuming it —
    // halves the requests without reintroducing the assumption that broke the first fix.
    const t = 1_700_000_000_000;
    const all = Array.from({ length: 30 }, (_, i) => record(t - i * 60_000, `id-${i}`));
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) =>
      all
        .filter((r) => r.timestamp >= startTime && r.timestamp <= endTime)
        .sort((a, b) => a.timestamp - b.timestamp)
        .slice(0, limit),
    );

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 60 * 60_000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 10,
    });

    expect(result.records).toHaveLength(30);
    expect(result.truncated).toBe(false);
    // 3 pages of 10 + 1 empty probe past the end. A both-sides split would issue more.
    expect(result.requests).toBeLessThanOrEqual(5);
  });

  it('falls back to BOTH sides when the page order is not monotonic', async () => {
    // An unsorted page tells us nothing about which end was served, so ambiguity must widen
    // the search rather than resolve to a guess. Being unsure costs a request; being
    // confidently wrong costs rows.
    const t = 1_700_000_000_000;
    const all = [record(t, 'a'), record(t - 5000, 'b'), record(t - 2000, 'c'), record(t - 9000, 'd')];
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) =>
      // Deliberately unsorted.
      all.filter((r) => r.timestamp >= startTime && r.timestamp <= endTime).slice(0, limit),
    );

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 20_000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 4,
    });

    expect(result.records).toHaveLength(4);
    expect(result.truncated).toBe(false);
  });

  it('paces between CHUNKS, not between pages inside one', async () => {
    // The delay is a weight budget: /fapi/v1/income is weight 30 against a 2400/min cap, so
    // binance's own maxRPS-3 request limiter does NOT satisfy it. Pacing every page instead
    // of every chunk would triple a 90-day backfill for no extra safety.
    vi.useFakeTimers();
    try {
      const t = 7 * 24 * 60 * 60 * 1000;
      const fetchPage = vi.fn(async () => []);
      const promise = paginateIncomeRange(fetchPage, {
        startTime: 0,
        endTime: t * 2,
        chunkMs: t,
        pageLimit: 100,
        chunkDelayMs: 2000,
      });

      await vi.advanceTimersByTimeAsync(0);
      expect(fetchPage).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1999);
      expect(fetchPage).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1);
      expect(fetchPage).toHaveBeenCalledTimes(2);
      await promise;
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('paginateIncomeRange — a failed page is not an empty one', () => {
  it('REPORTS truncation when a page throws instead of treating it as exhausted', async () => {
    // Found live, not theorised: the binance probe hit a 2400/min IP limit mid-walk and
    // returned 492 of 512 records while reporting success, because the errored pages came
    // back as `[]` and read as "nothing more here". An error must never become data.
    const t = 1_700_000_000_000;
    const all = Array.from({ length: 40 }, (_, i) => record(t - i * 60_000, `id-${i}`));
    let calls = 0;
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) => {
      calls += 1;
      if (calls === 2) throw new Error('429 Too many requests');
      return all
        .filter((r) => r.timestamp >= startTime && r.timestamp <= endTime)
        .sort((a, b) => a.timestamp - b.timestamp)
        .slice(0, limit);
    });

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 60 * 60_000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 10,
    });

    // The walk cannot claim completeness, and says so rather than returning a short ledger
    // that would reconcile perfectly well against itself.
    expect(result.truncated).toBe(true);
    expect(result.exhaustedChunks).toBe(1);
  });

  it('still returns the records it did retrieve', async () => {
    const t = 1_700_000_000_000;
    const all = Array.from({ length: 40 }, (_, i) => record(t - i * 60_000, `id-${i}`));
    let calls = 0;
    const fetchPage = vi.fn(async ({ startTime, endTime, limit }: any) => {
      calls += 1;
      if (calls > 2) throw new Error('429');
      return all
        .filter((r) => r.timestamp >= startTime && r.timestamp <= endTime)
        .sort((a, b) => a.timestamp - b.timestamp)
        .slice(0, limit);
    });

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - 60 * 60_000,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 10,
    });

    expect(result.records.length).toBeGreaterThan(0);
    expect(result.truncated).toBe(true);
  });
});

describe('paginateIncomeRange — records sharing a ledger row', () => {
  it('KEEPS both records a single ledger row produces', async () => {
    // Bitget emits a realized-PnL record AND a commission record from ONE bill, both
    // carrying the same billId. Keying dedupe on tranId alone discarded the commission from
    // every trade — 820 records where ~1190 were expected. The live paging probe could not
    // see it: it compares two walks through the same mapper, and both lost the same rows.
    const t = 1_700_000_000_000;
    const pair: IncomeRecord[] = [
      { symbol: 'TRXUSDT', incomeType: 'close_long', income: -0.00084, asset: 'USDT', timestamp: t, tranId: 'bill-1', category: 'realized_pnl' },
      { symbol: 'TRXUSDT', incomeType: 'close_long_fee', income: -0.00595, asset: 'USDT', timestamp: t, tranId: 'bill-1', category: 'commission' },
    ];
    const fetchPage = vi.fn(async ({ endTime }: any) => (endTime >= t ? pair : []));

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - DAY,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(2);
    expect(result.records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
  });

  it('still collapses the SAME record served twice', async () => {
    // The property that made tranId attractive must survive the fix.
    const t = 1_700_000_000_000;
    const one: IncomeRecord = { symbol: 'TRXUSDT', incomeType: 'close_long', income: -1, asset: 'USDT', timestamp: t, tranId: 'bill-9', category: 'realized_pnl' };
    const fetchPage = vi.fn(async ({ endTime }: any) => (endTime >= t ? [one, { ...one }] : []));

    const result = await paginateIncomeRange(fetchPage, {
      startTime: t - DAY,
      endTime: t + 1,
      chunkMs: HUNDRED_DAYS,
      pageLimit: 100,
    });

    expect(result.records).toHaveLength(1);
  });
});
