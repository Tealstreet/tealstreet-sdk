import type { Execution } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

it('Binance USD-M journal folds per-symbol fills like the TS venue', async () => {
  const exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
  adoptEngineCapabilities(exchange);
  const fold = vi
    .spyOn(
      exchange as unknown as { foldClosedPositionRecords: (...args: unknown[]) => Promise<unknown[]> },
      'foldClosedPositionRecords',
    )
    .mockResolvedValue([]);
  try {
    await expect(exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 })).resolves.toEqual([]);
    expect(fold).toHaveBeenCalledWith({ account: 'acct', from: 1, to: 2 }, { symbolScoped: true });
  } finally {
    exchange.dispose();
  }
});

it('drains a busy lookback through the public Binance positions read', async () => {
  const day = 86_400_000;
  const from = 1_700_000_000_000;
  const exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
  adoptEngineCapabilities(exchange);
  const rows: Execution[] = Array.from({ length: 1600 }, (_, index) => ({
    id: String(index),
    symbol: 'BTCUSDT',
    timestamp: (from - (index < 800 ? 10 : 9) * day + index * 1000) / 1000,
    price: index % 2 ? 110 : 100,
    qty: 1,
    side: index % 2 ? OrderSide.Sell : OrderSide.Buy,
    realizedPnl: index % 2 ? 10 : 0,
  }));
  rows.push(
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
  vi.spyOn(exchange, 'fetchIncomeHistoryRange').mockResolvedValue([
    { symbol: 'BTCUSDT', incomeType: 'FUNDING_FEE', income: 0, asset: 'USDT', timestamp: from, tranId: 'income' },
  ]);
  const read = vi.spyOn(exchange, 'fetchHistoricalFills').mockImplementation(async (opts) => {
    const { from, to } = opts;
    if (from === undefined || to === undefined) throw new Error('expected a bounded fill read');
    const selected = rows.filter((row) => row.timestamp * 1000 >= from && row.timestamp * 1000 <= to);
    // Physical page draining belongs to the native range reader.
    return selected;
  });
  try {
    const records = await exchange.fetchHistoricalPositions({ account: 'main', from, to: from + day });
    expect(records).toHaveLength(1);
    expect(records[0].realizedPnl).toBe(10);
    expect(read).toHaveBeenCalledTimes(1);
    expect(read).toHaveBeenCalledWith({ symbol: 'BTCUSDT', from: from - 14 * day, to: from + day, limit: 1000 });
  } finally {
    exchange.dispose();
  }
});

it('keeps sparse five-minute Binance history on one native read root', async () => {
  const from = 1_700_000_000_000;
  const to = from + 5 * 60 * 1000;
  const exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
  adoptEngineCapabilities(exchange);
  vi.spyOn(exchange, 'fetchIncomeHistoryRange').mockResolvedValue([
    { symbol: 'BTCUSDT', incomeType: 'FUNDING_FEE', income: 0, asset: 'USDT', timestamp: from, tranId: 'income' },
  ]);
  const fills: Execution[] = [
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
  ];
  const read = vi
    .spyOn(exchange, 'fetchHistoricalFills')
    .mockImplementation(async (range) =>
      fills.filter(
        (row) =>
          range.from !== undefined &&
          range.to !== undefined &&
          row.timestamp * 1000 >= range.from &&
          row.timestamp * 1000 <= range.to,
      ),
    );
  try {
    const records = await exchange.fetchHistoricalPositions({ account: 'main', from, to });
    expect(records).toHaveLength(1);
    expect(records[0].realizedPnl).toBe(10);
    expect(read).toHaveBeenCalledTimes(1);
    expect(read).toHaveBeenCalledWith({ symbol: 'BTCUSDT', from: from - 14 * 86_400_000, to, limit: 1000 });
  } finally {
    exchange.dispose();
  }
});
