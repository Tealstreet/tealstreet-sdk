import { describe, expect, it } from 'vitest';

import type { ExchangeName } from '../types';
import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

import { aggregateJournalRecords, caveatsForMetric, isMetricExact } from './aggregate';

/**
 * The point of this file: a caveat must land on the metrics it actually invalidates and on
 * NO others. bybit's money is right and its trade count is not, so a model that taints both
 * or neither is wrong in one direction or the other.
 */

const record = (over: Partial<PositionRecord> & { exchange: ExchangeName }) =>
  ({
    key: 'k',
    account: 'a',
    symbol: 'BTCUSDT',
    settleCurrency: 'USDT',
    side: PositionSide.Long,
    positionSide: 'both',
    state: 'closed',
    closedSize: 1,
    avgEntryPrice: 100,
    avgExitPrice: 110,
    realizedPnl: 10,
    fees: 1,
    funding: 0,
    fundingAttributed: true,
    provenance: 'reported',
    sourceAxis: 'fills',
    closeCount: 1,
    openedAt: 1_000,
    closedAt: 2_000,
    ...over,
  }) as PositionRecord & { exchange: ExchangeName };

describe('aggregateJournalRecords', () => {
  it('sums value-derived metrics across venues with NO caveat, including bybit', () => {
    // Merging round trips CONSERVES VALUE, so PnL/fees/funding are unaffected by bybit's
    // defect. Tainting them would be as wrong as ignoring the count problem.
    const result = aggregateJournalRecords([
      record({ exchange: 'bybit' as ExchangeName, realizedPnl: 10, fees: 1 }),
      record({ exchange: 'okx' as ExchangeName, realizedPnl: 5, fees: 2 }),
    ]);

    expect(result.realizedPnl).toBe(15);
    expect(result.fees).toBe(3);
    expect(result.netPnl).toBe(12);
    expect(caveatsForMetric(result, 'tradeCount')).toHaveLength(1);
    // The value totals carry no metric caveat at all.
    expect(result.caveats.every((c) => c.metric !== ('realizedPnl' as never))).toBe(true);
  });

  it('carries bybit’s caveat onto EVERY count-derived metric', () => {
    const result = aggregateJournalRecords([record({ exchange: 'bybit' as ExchangeName })]);

    for (const metric of ['tradeCount', 'winRate', 'avgPnlPerTrade', 'avgHoldTimeMs'] as const) {
      expect(isMetricExact(result, metric)).toBe(false);
      expect(caveatsForMetric(result, metric)[0]?.exchange).toBe('bybit');
    }
  });

  it('leaves count metrics exact when no contributing venue is caveated', () => {
    const result = aggregateJournalRecords([
      record({ exchange: 'okx' as ExchangeName }),
      record({ exchange: 'binance' as ExchangeName }),
    ]);
    expect(result.caveats).toHaveLength(0);
    expect(isMetricExact(result, 'tradeCount')).toBe(true);
  });

  it('does NOT taint the whole aggregate when one venue of many is caveated', () => {
    // The failure this guards: a single caveated venue making every number on the page
    // hedged, which trains users to ignore the badge entirely.
    const result = aggregateJournalRecords([
      record({ exchange: 'bybit' as ExchangeName }),
      record({ exchange: 'okx' as ExchangeName }),
    ]);
    expect(isMetricExact(result, 'tradeCount')).toBe(false);
    expect(result.realizedPnl).toBe(20);
  });

  it('reports ungraded venues separately from caveats — different claims', () => {
    // paradex is ungraded but NOT caveated: nothing says its counts are wrong, only that
    // nothing independent confirms its totals. Collapsing the two would misrepresent both.
    const result = aggregateJournalRecords([record({ exchange: 'paradex' as ExchangeName })]);
    expect(result.ungradedExchanges).toEqual(['paradex']);
    expect(result.caveats).toHaveLength(0);
  });

  it('excludes a record with no usable duration from the hold-time average', () => {
    // Counting it as zero would drag the mean down and read as a change in trading behaviour.
    const result = aggregateJournalRecords([
      record({ exchange: 'okx' as ExchangeName, openedAt: 1_000, closedAt: 3_000 }),
      record({ exchange: 'okx' as ExchangeName, openedAt: undefined, closedAt: undefined }),
    ]);
    expect(result.avgHoldTimeMs).toBe(2_000);
    expect(result.tradeCount).toBe(2);
  });

  it('excludes a BREAKEVEN trade from the win rate instead of counting it as a loss', () => {
    // Found on the first run against real data: 35 positive, 493 negative, 19 EXACTLY ZERO.
    // Counting those as losses reports a worse win rate for an account that simply scratched
    // more trades — a claim about skill made out of a modelling choice.
    const result = aggregateJournalRecords([
      record({ exchange: 'okx' as ExchangeName, realizedPnl: 10 }),
      record({ exchange: 'okx' as ExchangeName, realizedPnl: -5 }),
      record({ exchange: 'okx' as ExchangeName, realizedPnl: 0 }),
    ]);
    expect(result.winRate).toBe(0.5);
    expect(result.breakevenCount).toBe(1);
    // The breakeven still counts as a TRADE — it happened.
    expect(result.tradeCount).toBe(3);
  });

  it('returns undefined win rate when every trade was a scratch', () => {
    const result = aggregateJournalRecords([record({ exchange: 'okx' as ExchangeName, realizedPnl: 0 })]);
    expect(result.winRate).toBeUndefined();
    expect(result.breakevenCount).toBe(1);
  });

  it('returns undefined rather than NaN or 0 for an empty set', () => {
    const result = aggregateJournalRecords([]);
    expect(result.tradeCount).toBe(0);
    expect(result.winRate).toBeUndefined();
    expect(result.avgPnlPerTrade).toBeUndefined();
    expect(result.realizedPnl).toBe(0);
  });

  it('subtracts fees, which are positive-when-charged by canonical convention', () => {
    // That convention is PROVEN against bitget's own running balance, not assumed.
    const result = aggregateJournalRecords([
      record({ exchange: 'okx' as ExchangeName, realizedPnl: 10, fees: 3, funding: -1 }),
    ]);
    expect(result.netPnl).toBe(6);
  });
});
