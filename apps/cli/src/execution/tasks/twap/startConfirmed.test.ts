import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.exchange';
import { OrderSide, type Market } from '@tealstreet/safe-cex/src/types';

import { confirmedTwapHistory, confirmedTwapHosts, quiesceConfirmedTwaps, startConfirmedTwap } from './startConfirmed';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  info: {},
} as Market;

const store = {
  subscribe: vi.fn(),
  markets: [market],
  marketsBySymbol: { BTCUSDT: market },
  tickersBySymbol: { BTCUSDT: { last: 50_000, mark: 50_000 } },
  orders: [],
  positions: [],
  balance: {},
} as never;

const input = (exchange: BybitExchange) => ({
  accountName: 'bybit-main',
  exchange,
  market,
  side: OrderSide.Buy,
  totalSize: 0.01,
  slices: 1,
  durationMs: 60_000,
  explicitLegacy: false,
});

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_000);
});

afterEach(() => {
  confirmedTwapHosts.clear();
  confirmedTwapHistory.clear();
  vi.useRealTimers();
});

describe('confirmed TWAP session composition', () => {
  it('preserves scientific-notation contract sizes when converting native input to base', async () => {
    const scientificMarket = {
      ...market,
      contractSize: 1e-7,
      precision: { amount: 1, price: 0.1 },
      limits: { amount: { min: 1e-7 }, leverage: { min: 1, max: 100 } },
    } as Market;
    const scientificStore = {
      subscribe: vi.fn(),
      markets: [scientificMarket],
      marketsBySymbol: { BTCUSDT: scientificMarket },
      tickersBySymbol: { BTCUSDT: { last: 50_000, mark: 50_000 } },
      orders: [],
      positions: [],
      balance: {},
    } as never;
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, scientificStore);
    exchange.placeOrder = vi.fn().mockResolvedValue(['venue-order']);

    const result = await startConfirmedTwap({
      ...input(exchange),
      market: scientificMarket,
      totalSize: 10,
      sizeIsNative: true,
    });
    expect(result.engine).toBe('confirmed');

    await vi.advanceTimersByTimeAsync(1);
    expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 10 }));
  });

  it('keeps legacy outside, shares one account host, then fences every later admission', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);

    await expect(startConfirmedTwap({ ...input(exchange), explicitLegacy: true })).resolves.toEqual({
      engine: 'legacy',
      reason: 'explicit-legacy',
    });
    expect(confirmedTwapHosts.size).toBe(0);

    const first = await startConfirmedTwap(input(exchange));
    const second = await startConfirmedTwap(input(exchange));
    expect(first.engine).toBe('confirmed');
    expect(second.engine).toBe('confirmed');
    expect(confirmedTwapHosts.size).toBe(1);
    expect(confirmedTwapHosts.get('bybit-main')?.list()).toHaveLength(2);

    await expect(quiesceConfirmedTwaps(1_000)).resolves.toHaveLength(2);
    expect(confirmedTwapHosts.size).toBe(0);
    expect(confirmedTwapHistory.size).toBe(2);
    await expect(startConfirmedTwap(input(exchange))).resolves.toEqual({
      engine: 'error',
      message: 'Confirmed TWAP shutdown is in progress; no new task can start',
    });
  });
});
