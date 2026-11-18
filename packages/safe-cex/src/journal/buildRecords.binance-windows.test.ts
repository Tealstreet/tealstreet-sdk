import type { Execution } from '../types';

import { expect, it, vi } from 'vitest';

import { OrderSide } from '../types';
import { buildFoldedPositionRecords } from './buildRecords';

it('pages the saturated Binance lookback independently of the target window', async () => {
  const day = 86_400_000;
  const from = 1_700_000_000_000;
  const to = from + day;
  const source: Execution[] = [];
  for (let i = 0; i < 1600; i++) {
    source.push({
      id: String(i),
      symbol: 'BTCUSDT',
      timestamp: (from - (i < 800 ? 10 : 9) * day + i * 1000) / 1000,
      price: i % 2 ? 110 : 100,
      qty: 1,
      side: i % 2 ? OrderSide.Sell : OrderSide.Buy,
      realizedPnl: i % 2 ? 10 : 0,
    });
  }
  source.push(
    { id: 'entry', symbol: 'BTCUSDT', timestamp: (from + 1000) / 1000, price: 100, qty: 1, side: OrderSide.Buy },
    {
      id: 'exit',
      symbol: 'BTCUSDT',
      timestamp: (from + 2000) / 1000,
      price: 110,
      qty: 1,
      side: OrderSide.Sell,
      realizedPnl: 10,
    },
  );
  const fetchHistoricalFills = vi.fn(async ({ from: a, to: b }: { from: number; to: number }) => {
    const rows = source.filter((row) => row.timestamp * 1000 >= a && row.timestamp * 1000 <= b);
    return rows.length > 1000 ? Object.assign(rows.slice(0, 1000), { incomplete: true }) : rows;
  });
  const result = await buildFoldedPositionRecords(
    { fetchHistoricalFills, fetchIncomeHistoryRange: async () => [{ symbol: 'BTCUSDT' }] },
    { account: 'main', from, to, symbolScoped: true, maxFillWindowMs: day, resolveSettleCurrency: () => 'USDT' },
  );
  expect(result.totalFills).toBe(1602);
  expect(result.records).toHaveLength(1);
  expect(result.records[0].realizedPnl).toBe(10);
  expect(fetchHistoricalFills.mock.calls.every(([range]) => range.to - range.from < day)).toBe(true);
});

it('reconstructs a position once after joining fills across lookback windows', async () => {
  const day = 86_400_000;
  const from = 1_700_000_000_000;
  const source: Execution[] = [
    { id: 'entry', symbol: 'BTCUSDT', timestamp: (from - day) / 1000, price: 100, qty: 1, side: OrderSide.Buy },
    {
      id: 'exit',
      symbol: 'BTCUSDT',
      timestamp: (from + 1000) / 1000,
      price: 110,
      qty: 1,
      side: OrderSide.Sell,
      realizedPnl: 10,
    },
  ];
  const result = await buildFoldedPositionRecords(
    {
      fetchHistoricalFills: async ({ from: a, to: b }) =>
        source.filter((row) => row.timestamp * 1000 >= a && row.timestamp * 1000 <= b),
      fetchIncomeHistoryRange: async () => [{ symbol: 'BTCUSDT' }],
    },
    {
      account: 'main',
      from,
      to: from + day,
      symbolScoped: true,
      maxFillWindowMs: day,
      resolveSettleCurrency: () => 'USDT',
    },
  );
  expect(result.records).toHaveLength(1);
  expect(result.totalFills).toBe(2);
  expect(result.records[0]).toMatchObject({ openedAt: from - day, closedAt: from + 1000, realizedPnl: 10 });
});
