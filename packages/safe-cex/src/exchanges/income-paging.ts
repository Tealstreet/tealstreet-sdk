import type { IncomeRecord } from '../types';

/**
 * SHARED income-range paging — completeness decided by SPAN, not by a full page.
 *
 * Used by weex and binance. The two arrived here from opposite directions, which is why this
 * is one function rather than two:
 *
 * - **weex** issued exactly ONE request per 100-day chunk at `limit: 100`, no cursor at all.
 *   Silent truncation on any active account.
 * - **binance** genuinely paged, but gated BOTH ends of the loop on `length === 1000`
 *   (enter) and `length < 1000` (stop). A server-side filter or a cap below the asked limit
 *   returns a short page with more behind it, and the walk stops early — silently.
 *
 * Binance is the more dangerous of the two: it currently PASSES the canonical gate, so a
 * silent truncation there corrupts a GREEN verdict rather than producing an absent one. It
 * has not fired yet only because the test account is small enough that no 7-day window
 * reaches 1000 records — the pager is not right, the account is quiet.
 *
 * THREE THINGS THIS DOES NOT DO, each because the obvious version is subtly wrong:
 *
 * 1. **It does not infer completeness from a full page.** `page.length === limit` is the
 *    usual idiom and it is a guess about the venue's behaviour. Completeness is decided by
 *    whether the requested SPAN has been covered, which is the thing actually being asked.
 *
 * 2. **It does not assume a sort order — and does not try to detect one.** A backwards time
 *    cursor (the first version of this file) works perfectly against a newest-first venue and
 *    terminates after ONE page against an oldest-first one — reintroducing the original bug
 *    wearing a paging costume. Each answered page instead splits the window into the parts it
 *    did not cover, on BOTH sides. Whichever end the venue served, the other is still queued;
 *    the cost of not knowing is one empty request per branch, not silent data loss.
 *
 *    This matters even where the order IS known: binance returns ascending today, and a
 *    pager that depends on that is one API revision from losing data quietly.
 *
 * 3. **It does not pretend a time cursor can always make progress.** When more records share
 *    a single millisecond than fit in one page, no purely time-based cursor can reach past
 *    them — neither venue's endpoint offers an offset or id cursor. That case is REPORTED via
 *    `truncated` rather than papered over, because the alternative is a short ledger that
 *    reconciles.
 *
 * The loop is a pure function over a `fetchPage` callback so it can be exercised against
 * fixtures (weex has no credential anywhere in the sheet) as well as live (binance does).
 */

export type IncomePageFetcher = (params: {
  startTime: number;
  endTime: number;
  limit: number;
}) => Promise<IncomeRecord[]>;

export interface IncomeRangeOptions {
  startTime: number;
  endTime: number;
  /** Widest window the venue accepts in one request. */
  chunkMs: number;
  /** Rows per request. weex caps this at 100, binance at 1000. */
  pageLimit: number;
  /**
   * Awaited between CHUNKS — not between pages within a chunk.
   *
   * This is a weight budget, not politeness, and the client-side RPS limiter does not
   * satisfy it: `/fapi/v1/income` costs **weight 30** against binance's 2400/min IP budget,
   * so binance's own `maxRPS: 3` would be 5400 weight/min — over the cap. Dropping this in
   * favour of "the axios rate limiter already paces us" was wrong, and the pacing regression
   * caught it.
   */
  chunkDelayMs?: number;
  /**
   * Hard stop per chunk, so a venue that ignores the window cannot spin forever. Reaching it
   * sets `truncated`; it is never swallowed.
   */
  maxPagesPerChunk?: number;
}

export interface IncomeRangeResult {
  records: IncomeRecord[];
  /**
   * TRUE when the walk could not prove it covered the requested span.
   *
   * This field is the whole difference from the old behaviour. A silently partial ledger is
   * indistinguishable from an idle account, and a caller reconciling totals MUST be able to
   * tell "there were no more records" from "we stopped being able to ask".
   */
  truncated: boolean;
  /** Chunks that ended incomplete, for diagnostics. */
  exhaustedChunks: number;
  /**
   * Requests actually issued. The two-sided split trades extra requests for not assuming a
   * sort order, so this is the number to watch when the cost of that trade is questioned.
   */
  requests: number;
}

const DEFAULT_MAX_PAGES_PER_CHUNK = 50;

/** Identity for dedupe. Window branches overlap at their edges by construction. */
/**
 * Identity for dedupe. Branches overlap at their edges by construction, so the same record is
 * legitimately served more than once and must collapse — but records that merely SHARE A
 * LEDGER ROW must not.
 *
 * `tranId` alone is wrong, and bitget proves it: one bill row yields a realized-PnL record
 * AND a commission record carrying the same `billId`, so keying on it discarded the
 * commission from every trade — 820 records where ~1190 were expected, half the ledger gone.
 *
 * **The live paging probe could not see this**, because it compares two walks through the
 * same mapper and both lost the same rows. It was caught by arithmetic against the venue's
 * own row census instead, which is why that census is worth keeping.
 */
const identify = (record: IncomeRecord): string =>
  record?.tranId
    ? `${record.tranId}|${record.category ?? ''}|${record.incomeType ?? ''}|${record.income ?? ''}`
    : // A shared empty id would collapse every untagged row into one.
      `${record?.symbol ?? ''}|${record?.incomeType ?? ''}|${record?.timestamp ?? ''}|${record?.income ?? ''}`;

/**
 * Is this page sorted, and which way? Read from the rows as returned — never assumed.
 *
 * `'unknown'` for anything ambiguous (fewer than two usable rows, ties only, or a page that
 * is not monotonic), which makes the caller cover BOTH sides. Being unsure costs one extra
 * request; being confidently wrong costs rows, so ambiguity must never resolve to a guess.
 */
const pageDirection = (
  page: IncomeRecord[],
  rangeStart: number,
  rangeEnd: number,
): 'ascending' | 'descending' | 'unknown' => {
  const times: number[] = [];
  for (const record of page) {
    const timestamp = Number(record?.timestamp);
    if (Number.isFinite(timestamp) && timestamp >= rangeStart && timestamp <= rangeEnd) times.push(timestamp);
  }
  if (times.length < 2) return 'unknown';

  let ascending = true;
  let descending = true;
  for (let i = 1; i < times.length; i += 1) {
    if (times[i] < times[i - 1]) ascending = false;
    if (times[i] > times[i - 1]) descending = false;
  }
  // All-equal timestamps satisfy both and distinguish nothing.
  if (ascending === descending) return 'unknown';
  return ascending ? 'ascending' : 'descending';
};

export const paginateIncomeRange = async (
  fetchPage: IncomePageFetcher,
  opts: IncomeRangeOptions,
): Promise<IncomeRangeResult> => {
  const { startTime, endTime, chunkMs, pageLimit } = opts;
  const maxPages = opts.maxPagesPerChunk ?? DEFAULT_MAX_PAGES_PER_CHUNK;

  const byId = new Map<string, IncomeRecord>();
  let truncated = false;
  let exhaustedChunks = 0;
  let requests = 0;

  if (!(endTime > startTime) || !(chunkMs > 0) || !(pageLimit > 0)) {
    return { records: [], truncated: false, exhaustedChunks: 0, requests: 0 };
  }

  let firstChunk = true;

  for (let chunkStart = startTime; chunkStart < endTime; chunkStart += chunkMs) {
    const chunkEnd = Math.min(chunkStart + chunkMs, endTime);

    if (!firstChunk && opts.chunkDelayMs) {
      await new Promise((resolve) => setTimeout(resolve, opts.chunkDelayMs));
    }
    firstChunk = false;

    // Sub-windows still to be proven empty. A page answers part of a window; the parts it
    // did not answer go back on the queue. No assumption about WHICH part it answered.
    const pending: Array<[number, number]> = [[chunkStart, chunkEnd]];
    let pages = 0;
    let chunkIncomplete = false;

    while (pending.length) {
      if (pages >= maxPages) {
        chunkIncomplete = true;
        break;
      }
      pages += 1;

      const [from, to] = pending.pop()!;
      if (from > to) continue;

      requests += 1;

      let page: IncomeRecord[];
      try {
        page = await fetchPage({ startTime: from, endTime: to, limit: pageLimit });
      } catch {
        // A FAILED page is not an empty one. If the fetcher throws — rate limit, transport,
        // auth — the rows behind it are unknown, not absent. Treating the two alike is how
        // the live probe lost 20 of 512 binance records to a 429 while reporting success,
        // so the branch is abandoned and the walk is marked incomplete rather than clean.
        chunkIncomplete = true;
        continue;
      }
      // The only clean termination for a branch: the venue says there is nothing here.
      if (!Array.isArray(page) || page.length === 0) continue;

      let oldest = Number.POSITIVE_INFINITY;
      let newest = Number.NEGATIVE_INFINITY;
      let insideWindow = 0;
      let fresh = 0;

      for (const record of page) {
        const timestamp = Number(record?.timestamp);
        if (!Number.isFinite(timestamp)) continue;
        // Only rows the caller actually asked for. A venue that ignores the window would
        // otherwise smuggle out-of-range rows into the result.
        if (timestamp < startTime || timestamp > endTime) continue;
        if (timestamp >= from && timestamp <= to) insideWindow += 1;
        if (timestamp < oldest) oldest = timestamp;
        if (timestamp > newest) newest = timestamp;

        const id = identify(record);
        if (!byId.has(id)) {
          byId.set(id, record);
          fresh += 1;
        }
      }

      // Either the timestamps are unusable, or every row sits outside the window that was
      // asked for — i.e. the venue is ignoring the range. Neither can be walked further, and
      // both would otherwise end the branch as if it were exhausted.
      if (!Number.isFinite(oldest) || insideWindow === 0) {
        chunkIncomplete = true;
        continue;
      }

      // A branch that yields nothing new cannot make progress on an INCLUSIVE boundary.
      //
      // A short page here means the branch is genuinely exhausted — the clean stop. A FULL
      // page means the boundary instant is saturated: more rows share that millisecond than
      // fit, and no time cursor can reach them. Those specific rows are lost and that is
      // reported — but the REST of the window is still reachable, so the walk continues past
      // the instant with an exclusive bound. Without this, one saturated millisecond silently
      // swallowed everything older than it.
      if (fresh === 0) {
        if (page.length < pageLimit) continue;
        chunkIncomplete = true;

        const stuck = pageDirection(page, startTime, endTime);
        if (stuck !== 'ascending' && oldest - 1 >= from) pending.push([from, oldest - 1]);
        if (stuck !== 'descending' && newest + 1 <= to) pending.push([newest + 1, to]);
        continue;
      }

      // Which end of the window did the venue actually serve?
      //
      // This READS the order out of the page rather than assuming one. A page returned in
      // ascending order holds the OLDEST rows of the window, so only the part above `newest`
      // can still hold data; descending is the mirror. A page that is not monotonic tells us
      // nothing, so both sides stay queued.
      //
      // The distinction from the bug this file exists to fix: that pager BAKED IN
      // newest-first and stopped after one page against an ascending venue. This one asks
      // the response every single time, and falls back to covering both sides the moment the
      // answer is unclear — cost is a wasted request, never a dropped row.
      //
      // BOUNDARIES ARE INCLUSIVE, and that is not an off-by-one. Stepping to `newest + 1`
      // skips every row that SHARES the boundary millisecond but did not fit in the page —
      // found live on binance, where a 25-row page cut a COMMISSION off a REALIZED_PNL
      // sharing its timestamp and the walk returned 279 of 280 records. Re-asking from
      // `newest` re-serves rows already held, which dedupe absorbs, and the no-fresh-rows
      // rule above is what terminates the branch.
      const direction = pageDirection(page, startTime, endTime);
      if (direction !== 'ascending' && oldest >= from) pending.push([from, oldest]);
      if (direction !== 'descending' && newest <= to) pending.push([newest, to]);
    }

    if (chunkIncomplete) {
      truncated = true;
      exhaustedChunks += 1;
    }
  }

  // Newest first, matching every other venue's range reader.
  const records = [...byId.values()].sort((a, b) => b.timestamp - a.timestamp);
  return { records, truncated, exhaustedChunks, requests };
};
