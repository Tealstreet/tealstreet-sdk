import type { Execution } from '../types';
import type { PositionInterval } from './position-intervals';
import type { PositionRecord } from './position.types';

import { mapFoldedInterval } from './fold.position-adapter';
import { attributeFunding, reconstructPositionIntervals } from './position-intervals';

/**
 * Folding a venue's fills into closed round trips.
 *
 * **Extracted so the app and the audit harness run ONE implementation.** The harness has been
 * folding records this way all week and grading them against venue ledgers; a second copy
 * written for the UI is exactly where the two would drift, and the drift would be invisible
 * because both would look correct alone — the same per-site inconsistency that let a breakeven
 * count as a loss ten lines from code that refused to do it.
 *
 * Deliberately NOT venue-aware. Every venue-specific decision already lives somewhere else:
 * whether a venue may be shown at all is decided once at the seam (`select.ts`), and how a
 * venue's fills map into `Execution` is its adapter's job. This function only folds.
 */

/** Straddle lookback: a position open when the window began has fills before it. */
const STRADDLE_LOOKBACK_MS = 14 * 24 * 60 * 60 * 1000;

export type BuildRecordsOptions = {
  readonly account: string;
  readonly from: number;
  readonly to: number;
  /**
   * Venues whose fills route demands a symbol (binance). An account-wide ask is one request
   * instead of N and is preferred wherever the venue allows it.
   *
   * Also decides whether a failed income read is fatal — see the throw below.
   */
  readonly symbolScoped?: boolean;
  readonly resolveSettleCurrency: (symbol: string) => string;
};

export type BuildRecordsResult = {
  readonly records: PositionRecord[];
  readonly totalFills: number;
  readonly intervals: PositionInterval[];
};

type HistoricalRows<T> = T[] & { readonly incomplete?: boolean };

const requireCompleteHistory = <T>(rows: HistoricalRows<T>, axis: 'fills' | 'income'): HistoricalRows<T> => {
  if (rows.incomplete) throw new Error(`journal fold: ${axis} history is incomplete`);
  return rows;
};

type FoldableExchange = {
  fetchHistoricalFills: (opts: {
    symbol?: string;
    from: number;
    to: number;
    limit: number;
  }) => Promise<HistoricalRows<Execution>>;
  fetchIncomeHistoryRange?: (
    from: number,
    to: number,
  ) => Promise<HistoricalRows<{ symbol?: string; category?: string; timestamp?: number; income?: number }>>;
};

export const buildFoldedPositionRecords = async (
  exchange: FoldableExchange,
  options: BuildRecordsOptions,
): Promise<BuildRecordsResult> => {
  const lookbackFrom = options.from - STRADDLE_LOOKBACK_MS;

  const fundingRows: Array<{ symbol: string; timestamp: number; income: number }> = [];
  const symbols = new Set<string>();

  // Symbols come from what the account ACTUALLY traded. Guessing a list silently measures a
  // subset, and a subset looks like a quiet account rather than a partial answer.
  let incomeFailure: { readonly error: unknown } | undefined;
  if (exchange.fetchIncomeHistoryRange) {
    try {
      const income = requireCompleteHistory(
        await exchange.fetchIncomeHistoryRange(lookbackFrom, options.to),
        'income',
      );
      for (const item of income) {
        if (item.symbol) symbols.add(String(item.symbol));
        if (item.category === 'funding') {
          fundingRows.push({
            symbol: String(item.symbol ?? ''),
            timestamp: Number(item.timestamp),
            income: Number(item.income),
          });
        }
      }
    } catch (error: unknown) {
      // Wrapped rather than assigned raw, so a venue that throws a falsy value is still a
      // failure. What it COSTS is decided below, and differs by an order of magnitude.
      incomeFailure = { error };
    }
  }

  // FATAL only where the symbol list DRIVES the fetch. Here it is the entire input to the
  // loop below, so a swallowed failure means zero fills and an account that traded reported
  // as one that did not — the confusion this whole surface exists to prevent.
  //
  // Account-wide venues fetch their fills regardless and lose only funding, which every
  // record already states as `fundingAttributed: false` rather than as a zero. Failing the
  // whole axis there would discard a complete dataset over a missing enrichment.
  if (incomeFailure && options.symbolScoped) {
    const failure = new Error(
      'journal fold: income read failed on a symbol-scoped venue, so the symbol list that ' +
        'drives the fills fetch is unknown — continuing would report an empty account',
      { cause: incomeFailure.error },
    );
    // Keep the venue's error NAME: the caller records only that, and a bare `Error` erases
    // the difference between a rate limit and a rejected key.
    if (incomeFailure.error instanceof Error) failure.name = incomeFailure.error.name;
    throw failure;
  }

  const intervals: PositionInterval[] = [];
  let totalFills = 0;

  if (options.symbolScoped) {
    for (const symbol of symbols) {
      const fills = requireCompleteHistory(
        await exchange.fetchHistoricalFills({ symbol, from: lookbackFrom, to: options.to, limit: 1000 }),
        'fills',
      );
      totalFills += fills.length;
      intervals.push(...reconstructPositionIntervals(fills));
    }
  } else {
    const fills = requireCompleteHistory(
      await exchange.fetchHistoricalFills({ from: lookbackFrom, to: options.to, limit: 1000 }),
      'fills',
    );
    totalFills = fills.length;

    // Split per symbol before folding: an interval reconstruction that mixed symbols would
    // net unrelated positions against each other and produce round trips that never happened.
    const bySymbol = new Map<string, Execution[]>();
    for (const fill of fills) {
      const list = bySymbol.get(fill.symbol) ?? [];
      list.push(fill);
      bySymbol.set(fill.symbol, list);
    }
    for (const list of bySymbol.values()) intervals.push(...reconstructPositionIntervals(list));
  }

  const records: PositionRecord[] = [];
  for (const interval of intervals) {
    // Records that closed BEFORE the requested window are lookback artefacts — they exist so
    // a straddling position has a complete entry price, not to be shown.
    if (interval.closedAt === undefined || interval.closedAt < options.from) continue;

    const record = mapFoldedInterval(interval, {
      account: options.account,
      resolveSettleCurrency: options.resolveSettleCurrency,
      resolveFunding: fundingRows.length ? (i) => attributeFunding(i, fundingRows, { hedged: false }) : undefined,
    });
    if (record) records.push(record);
  }

  return { records, totalFills, intervals };
};
