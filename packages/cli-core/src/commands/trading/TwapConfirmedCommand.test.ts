import { describe, expect, it, vi } from 'vitest';

import type { ExchangeLike } from '../../types';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';
import type { CommandContext } from '../Command';
import { TwapCommand } from './TwapCommand';

const exchange = {
  name: 'test',
  store: {
    markets: [
      {
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        precision: { amount: 0.001, price: 0.01 },
      },
      {
        symbol: 'OUSDT',
        base: 'O',
        quote: 'USDT',
        precision: { amount: 1, price: 0.0001 },
        contractSize: 0.01,
      },
      {
        symbol: 'BTCUSD',
        base: 'BTC',
        quote: 'USD',
        precision: { amount: 1, price: 0.1 },
        contractSize: 100,
        isInverse: true,
      },
    ],
    tickers: [
      { symbol: 'BTCUSDT', last: 50_000 },
      { symbol: 'OUSDT', last: 1 },
      { symbol: 'BTCUSD', last: 50_000 },
    ],
    orders: [],
    positions: [],
    balance: { used: 0, free: 1_000_000, total: 1_000_000, upnl: 0 },
  },
  emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
} as unknown as ExchangeLike;

const context = (args: string[], preprocessStore?: MemoryPreprocessStore): CommandContext =>
  ({
    raw: args.join(' '),
    args: { _: args },
    symbol: 'BTCUSDT',
    accountName: 'account',
    exchange,
    preprocessStore,
    adapter: {
      print: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
    },
  }) as unknown as CommandContext;

const innerParams = (result: Awaited<ReturnType<TwapCommand['execute']>>): Record<string, unknown> =>
  (result.taskAction?.params?.params ?? {}) as Record<string, unknown>;

describe('TwapCommand confirmed routing flags', () => {
  it('marks the explicit legacy spelling without changing the order request', async () => {
    const result = await new TwapCommand().execute(
      context(['twap', 'buy', '$1000', 'into', '5', 'over', '1', 'legacy']),
    );

    expect(result.success).toBe(true);
    expect(innerParams(result)).toMatchObject({
      totalSize: 0.02,
      slices: 5,
      duration: 60_000,
      legacy: true,
    });
  });

  it('leaves the default route eligible for confirmed admission', async () => {
    const result = await new TwapCommand().execute(
      context(['twap', 'sell', '$1000', 'into', '5', 'over', '1', 'reduce']),
    );

    expect(result.success).toBe(true);
    expect(innerParams(result)).toMatchObject({ legacy: false, reduceOnly: true });
  });

  it('preserves a bare base quantity for the compiled live-proof form', async () => {
    const result = await new TwapCommand().execute(context(['twap', 'buy', '0.0000001', 'into', '5', 'over', '1']));

    expect(result.success).toBe(true);
    expect(result.taskAction?.params?.symbol).toBe('BTCUSDT');
    expect(innerParams(result)).toMatchObject({
      totalSize: 0.0000001,
      slices: 5,
      duration: 60_000,
      legacy: false,
    });
  });

  it('applies fatfinger limits to venue-native contracts for a bare base quantity', async () => {
    const store = new MemoryPreprocessStore();
    store.setFatfinger('OUSDT', 10);

    const result = await new TwapCommand().execute(
      context(['twap', 'buy', '1', 'OUSDT', 'into', '1', 'over', '1'], store),
    );

    expect(result.success).toBe(false);
    expect(result.error).toMatch(/fatfinger.*100.*limit 10/);
  });

  it('rejects bare base syntax on an inverse market instead of mis-sizing legacy TWAP', async () => {
    const result = await new TwapCommand().execute(
      context(['twap', 'buy', '1', 'BTCUSD', 'into', '1', 'over', '1', 'legacy']),
    );

    expect(result.success).toBe(false);
    expect(result.error).toMatch(/Bare base quantities.*inverse TWAP market BTCUSD/);
  });
});
