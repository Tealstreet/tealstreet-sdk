import type { Execution } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../types';
import { buildFoldedPositionRecords } from './buildRecords';

/**
 * A FAILED INCOME READ IS NOT AN ACCOUNT THAT DID NOT TRADE.
 *
 * The income ledger is where the SYMBOL LIST comes from, and on a symbol-scoped venue that
 * list is the entire input to the fills fetch. Swallowing the failure produces zero fills,
 * zero records, and a result byte-identical to a quiet account — the same conflation that
 * left 43.2% of `users_app_closed_positions` carrying a fabricated `exitPrice: 0`.
 */

const FROM = 1_700_000_000_000;
const TO = FROM + 24 * 60 * 60 * 1000;

/** A buy then a sell, which folds into exactly one closed round trip. */
const roundTrip = (symbol: string): Execution[] => [
  { timestamp: FROM / 1000 + 60, symbol, price: 100, qty: 1, side: OrderSide.Buy, commission: 0.1 },
  {
    timestamp: FROM / 1000 + 120,
    symbol,
    price: 110,
    qty: 1,
    side: OrderSide.Sell,
    commission: 0.1,
    realizedPnl: 10,
  },
];

const options = (over: { symbolScoped?: boolean } = {}) => ({
  account: 'main',
  from: FROM,
  to: TO,
  resolveSettleCurrency: () => 'USDT',
  ...over,
});

describe('buildFoldedPositionRecords — a failed income read is not an empty account', () => {
  it('rejects on a symbol-scoped venue instead of reporting an account that did not trade', async () => {
    const fetchHistoricalFills = vi.fn(async () => roundTrip('BTCUSDT'));
    const exchange = {
      fetchHistoricalFills,
      fetchIncomeHistoryRange: async () => {
        throw new Error('429 Too Many Requests');
      },
    };

    await expect(buildFoldedPositionRecords(exchange, options({ symbolScoped: true }))).rejects.toThrow(
      /symbol-scoped/,
    );
    // Not merely a throw: the fills fetch must never have been attempted with a symbol list
    // we know is wrong, because a short list is what silently under-collects.
    expect(fetchHistoricalFills).not.toHaveBeenCalled();
  });

  it('keeps the venue error NAME, so a rate limit stays distinct from a rejected key', async () => {
    // The caller (`collectAccountAxes`) records `err.name` and nothing else — venue error
    // text embeds IPs and signed query strings — so a bare `Error` erases the diagnosis.
    const rejected = new Error('signature invalid');
    rejected.name = 'AuthError';

    const thrown = await buildFoldedPositionRecords(
      {
        fetchHistoricalFills: async () => [],
        fetchIncomeHistoryRange: async () => {
          throw rejected;
        },
      },
      options({ symbolScoped: true }),
    ).catch((err: unknown) => err);

    expect((thrown as Error).name).toBe('AuthError');
    expect((thrown as Error).cause).toBe(rejected);
  });

  it('still treats an income read that SUCCEEDED with no symbols as an empty account', async () => {
    // The distinction being defended runs both ways: a venue that answered "none" must not
    // be promoted into a failure any more than a failure may be demoted into an empty.
    const result = await buildFoldedPositionRecords(
      { fetchHistoricalFills: async () => [], fetchIncomeHistoryRange: async () => [] },
      options({ symbolScoped: true }),
    );

    expect(result.records).toEqual([]);
    expect(result.totalFills).toBe(0);
  });

  it('fetches one symbol per traded symbol when the income read succeeds', async () => {
    const fetchHistoricalFills = vi.fn(async ({ symbol }: { symbol?: string }) => roundTrip(symbol ?? 'BTCUSDT'));

    const result = await buildFoldedPositionRecords(
      {
        fetchHistoricalFills,
        fetchIncomeHistoryRange: async () => [{ symbol: 'BTCUSDT' }, { symbol: 'ETHUSDT' }],
      },
      options({ symbolScoped: true }),
    );

    expect(fetchHistoricalFills.mock.calls.map((c) => c[0].symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
    expect(result.records.map((r) => r.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
  });

  it('costs an account-wide venue its funding attribution, not its whole dataset', async () => {
    // Fills there are fetched without a symbol list, so the records are complete. Discarding
    // them over a missing enrichment would lose data the venue may not serve twice.
    const result = await buildFoldedPositionRecords(
      {
        fetchHistoricalFills: async () => roundTrip('BTCUSDT'),
        fetchIncomeHistoryRange: async () => {
          throw new Error('429 Too Many Requests');
        },
      },
      options({ symbolScoped: false }),
    );

    expect(result.records).toHaveLength(1);
    expect(result.records[0].realizedPnl).toBe(10);
    // The honest state for funding nobody could read — NOT a zero presented as a total.
    expect(result.records[0].fundingAttributed).toBe(false);
  });

  it('attributes funding on an account-wide venue when the income read succeeds', async () => {
    const result = await buildFoldedPositionRecords(
      {
        fetchHistoricalFills: async () => roundTrip('BTCUSDT'),
        fetchIncomeHistoryRange: async () => [
          { symbol: 'BTCUSDT', category: 'funding', timestamp: FROM + 90_000, income: -0.5 },
        ],
      },
      options({ symbolScoped: false }),
    );

    expect(result.records[0].funding).toBe(-0.5);
    expect(result.records[0].fundingAttributed).toBe(true);
  });
});
