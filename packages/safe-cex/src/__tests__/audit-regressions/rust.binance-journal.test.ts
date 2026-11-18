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
    return selected.length > 1000 ? Object.assign(selected.slice(0, 1000), { incomplete: true }) : selected;
  });
  try {
    const records = await exchange.fetchHistoricalPositions({ account: 'main', from, to: from + day });
    expect(records).toHaveLength(1);
    expect(records[0].realizedPnl).toBe(10);
    expect(
      read.mock.calls.every(
        ([range]) => range.from !== undefined && range.to !== undefined && range.to - range.from < day,
      ),
    ).toBe(true);
  } finally {
    exchange.dispose();
  }
});
