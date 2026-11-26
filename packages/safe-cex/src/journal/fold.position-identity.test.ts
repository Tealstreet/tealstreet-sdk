import type { Execution } from '../types';

import { describe, expect, it } from 'vitest';

import { OrderSide } from '../types';
import { buildFoldedPositionRecords } from './buildRecords';

const SECOND = 1_700_000_000;

// Binance's native reader floors execution milliseconds to seconds. These are two
// complete round trips, not two partial closes of one position. Numeric trade IDs
// retain their order even when the endpoint returns the fills newest first.
const fills: Execution[] = [
  { id: '101', symbol: 'BTCUSDT', timestamp: SECOND, side: OrderSide.Buy, price: 100, qty: 1, realizedPnl: 0 },
  { id: '102', symbol: 'BTCUSDT', timestamp: SECOND, side: OrderSide.Sell, price: 110, qty: 1, realizedPnl: 10 },
  { id: '103', symbol: 'BTCUSDT', timestamp: SECOND, side: OrderSide.Buy, price: 110, qty: 1, realizedPnl: 0 },
  { id: '104', symbol: 'BTCUSDT', timestamp: SECOND, side: OrderSide.Sell, price: 105, qty: 1, realizedPnl: -5 },
];

const build = (rows: Execution[]) =>
  buildFoldedPositionRecords(
    {
      fetchHistoricalFills: async () => rows,
      fetchIncomeHistoryRange: async () => [{ symbol: 'BTCUSDT' }],
    },
    {
      account: 'main',
      from: SECOND * 1000,
      to: (SECOND + 1) * 1000,
      symbolScoped: true,
      resolveSettleCurrency: () => 'USDT',
    },
  );

describe('folded position identity at shared native timestamps (TEAL-7 B2)', () => {
  it('keeps two complete round trips in one second as two independently upsertable records', async () => {
    const { records } = await build([...fills].reverse());

    expect(records).toHaveLength(2);
    expect(records.map((record) => [record.openedAt, record.closedAt])).toEqual([
      [SECOND * 1000, SECOND * 1000],
      [SECOND * 1000, SECOND * 1000],
    ]);
    expect(records.map((record) => record.realizedPnl)).toEqual([10, -5]);
    expect(new Set(records.map((record) => record.key)).size).toBe(2);

    // A later fetch can contain either round trip alone. Its identity must not
    // depend on its ordinal in the response or on the requested window.
    const first = await build(fills.slice(0, 2));
    const second = await build(fills.slice(2));
    expect(first.records.map((record) => record.key)).toEqual([records[0].key]);
    expect(second.records.map((record) => record.key)).toEqual([records[1].key]);
    const stored = new Map([...records, ...second.records].map((record) => [record.key, record]));
    expect(stored.size).toBe(2);
    expect([...stored.values()].reduce((total, record) => total + record.realizedPnl, 0)).toBe(5);
  });

  it('keeps the close and opening halves of a flip distinct at the same timestamp', async () => {
    const { records, intervals } = await build([fills[0], { ...fills[1], qty: 2 }, { ...fills[2], realizedPnl: 0 }]);
    expect(records).toHaveLength(2);
    expect(intervals.map((interval) => interval.closedByFlip)).toEqual([true, false]);
    expect(new Set(records.map((record) => record.key)).size).toBe(2);
  });

  it('retains identity when a wider lookback reveals earlier opening fills', async () => {
    const full = await build([
      { ...fills[0], id: '100', timestamp: SECOND - 1, qty: 0.5 },
      { ...fills[0], qty: 0.5 },
      fills[1],
    ]);
    const narrow = await build(fills.slice(0, 2));
    expect(full.records[0].openedAt).not.toBe(narrow.records[0].openedAt);
    expect(full.records[0].key).toBe(narrow.records[0].key);
  });

  it('refuses an unidentifiable close instead of emitting a colliding key or silently losing the row', async () => {
    await expect(build([fills[0], { ...fills[1], id: undefined }])).rejects.toThrow('closing execution ID is required');
  });
});
