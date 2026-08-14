import type { ExchangeName } from '../types';
import type { PositionRecord } from './position.types';

import { getJournalVenueStatus, isIndependentlyReconciled } from './venue-grading';

/**
 * Aggregating journal records across venues, with the caveat carried PER METRIC.
 *
 * **The problem this exists to prevent.** bybit's totals reconcile but its trade COUNT does
 * not — two round trips reopening at an identical entry price merge into one, and merging
 * conserves value so no identity can see it. So a sum of realized PnL across venues is sound
 * while a sum of trade counts silently inherits an undercount, and every metric derived from
 * the count (win rate, average PnL per trade, average hold time) inherits it too.
 *
 * A per-VENUE caveat cannot express that: it would either taint the PnL total, which is
 * correct, or leave the trade count clean, which is not. So the caveat attaches to the
 * METRICS it actually invalidates, and the unsafe ones say so rather than averaging the error
 * away.
 *
 * This is the equity-curve problem arriving early. A curve built from these records is
 * value-derived and therefore safe under this caveat; a "trades per day" overlay on the same
 * curve is not, and would be wrong in a way nothing downstream could detect.
 */

/** Metrics a caveat can invalidate. Value-derived metrics are absent by construction. */
export type JournalMetric = 'tradeCount' | 'winRate' | 'avgPnlPerTrade' | 'avgHoldTimeMs';

/** Which metrics each caveat kind invalidates. */
const METRICS_INVALIDATED_BY: Record<string, readonly JournalMetric[]> = {
  // Merged round trips: N trades become N-1. Money is conserved; anything counting is not.
  'undercounts-trades': ['tradeCount', 'winRate', 'avgPnlPerTrade', 'avgHoldTimeMs'],
};

export type JournalCaveatNote = {
  readonly metric: JournalMetric;
  readonly exchange: ExchangeName;
  readonly text: string;
};

export type JournalAggregate = {
  /** VALUE-derived and safe under every caveat currently modelled. */
  readonly realizedPnl: number;
  readonly fees: number;
  readonly funding: number;
  readonly netPnl: number;

  /** COUNT-derived. Check `caveats` before presenting any of these as exact. */
  readonly tradeCount: number;
  readonly winRate?: number;
  readonly avgPnlPerTrade?: number;
  readonly avgHoldTimeMs?: number;

  /**
   * Non-empty when some contributing venue's caveat invalidates a metric above. The UI must
   * surface these on the affected metric — not as a footnote on the page, which is the same
   * as averaging it away.
   */
  readonly caveats: readonly JournalCaveatNote[];
  /**
   * Venues whose numbers rest on nothing outside their own data. NOT wrong — every per-record
   * invariant still ran — but no independent axis confirmed the totals.
   */
  readonly ungradedExchanges: readonly ExchangeName[];
};

const isWin = (record: PositionRecord) => record.realizedPnl > 0;

export const aggregateJournalRecords = (
  records: ReadonlyArray<PositionRecord & { exchange: ExchangeName }>,
): JournalAggregate => {
  let realizedPnl = 0;
  let fees = 0;
  let funding = 0;
  let wins = 0;
  let holdTimeTotal = 0;
  let holdTimeCount = 0;

  const contributing = new Set<ExchangeName>();

  for (const record of records) {
    realizedPnl += record.realizedPnl;
    fees += record.fees;
    funding += record.funding;
    if (isWin(record)) wins += 1;
    contributing.add(record.exchange);

    // Only records that report BOTH ends contribute a hold time. A record missing one is
    // excluded from the average rather than counted as zero duration, which would drag the
    // mean toward zero and look like a behaviour change in the user's trading.
    if (record.openedAt && record.closedAt && record.closedAt > record.openedAt) {
      holdTimeTotal += record.closedAt - record.openedAt;
      holdTimeCount += 1;
    }
  }

  const tradeCount = records.length;

  const caveats: JournalCaveatNote[] = [];
  const ungradedExchanges: ExchangeName[] = [];

  for (const exchange of contributing) {
    const status = getJournalVenueStatus(exchange);
    if (!isIndependentlyReconciled(exchange)) ungradedExchanges.push(exchange);
    if (!status.caveat) continue;

    for (const metric of METRICS_INVALIDATED_BY[status.caveat.kind] ?? []) {
      caveats.push({ metric, exchange, text: status.caveat.text });
    }
  }

  return {
    realizedPnl,
    fees,
    funding,
    // Fees are positive-when-charged by canonical convention — PROVEN against bitget's own
    // running balance, not assumed — so net subtracts them. Funding is signed already.
    netPnl: realizedPnl - fees + funding,
    tradeCount,
    winRate: tradeCount > 0 ? wins / tradeCount : undefined,
    avgPnlPerTrade: tradeCount > 0 ? realizedPnl / tradeCount : undefined,
    avgHoldTimeMs: holdTimeCount > 0 ? holdTimeTotal / holdTimeCount : undefined,
    caveats,
    ungradedExchanges,
  };
};

/** Every caveat affecting one metric, for rendering beside that number. */
export const caveatsForMetric = (aggregate: JournalAggregate, metric: JournalMetric): readonly JournalCaveatNote[] =>
  aggregate.caveats.filter((note) => note.metric === metric);

/** Whether a metric can be presented as exact. */
export const isMetricExact = (aggregate: JournalAggregate, metric: JournalMetric): boolean =>
  !aggregate.caveats.some((note) => note.metric === metric);
