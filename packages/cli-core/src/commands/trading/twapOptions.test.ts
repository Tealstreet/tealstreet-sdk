import { describe, it, expect } from 'vitest';
import { twapTaskParamsToOptions } from './twapOptions';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import type { Market } from '@tealstreet/safe-cex/src/types';
import type { TwapTaskParams } from '../../types';

const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: {
    amount: { min: 0.001, max: 1000 },
    leverage: { min: 1, max: 100 },
  },
} as Market;

function makeParams(overrides: Partial<TwapTaskParams> = {}): TwapTaskParams {
  return {
    totalSize: 0.01,
    duration: 1_800_000, // 30 min in ms — `twap buy $100 ... over 30`
    slices: 10,
    ordersExecuted: 0,
    ...overrides,
  };
}

const ctx = { accountName: 'a', market, side: OrderSide.Buy };

describe('twapTaskParamsToOptions — CLI ms → safe-cex minutes seam', () => {
  it('converts duration ms to minutes', () => {
    const options = twapTaskParamsToOptions(makeParams(), ctx);
    expect(options.duration).toBe(30);
  });

  it('yields the slice interval TwapInstance will compute (minutes × 60_000)', () => {
    const options = twapTaskParamsToOptions(makeParams(), ctx);
    // The exact quantity TwapInstance.sliceIntervalMs() computes:
    // duration (min) / ordersCount * 60_000. 30 min / 10 slices → 3 min.
    expect((options.duration / options.ordersCount) * 60_000).toBe(180_000);
  });

  it('converts 90 minutes in ms (mirrors the TwapChaseCommand parse)', () => {
    const options = twapTaskParamsToOptions(makeParams({ duration: 90 * 60_000 }), ctx);
    expect(options.duration).toBe(90);
  });

  it('maps task fields to runtime fields', () => {
    const options = twapTaskParamsToOptions(
      makeParams({
        totalSize: 0.5,
        slices: 4,
        reduceOnly: true,
        useChaserOrders: true,
        timeoutPct: 30,
        timeoutAction: 'next',
        failAction: 'market',
      }),
      ctx,
    );

    expect(options.size).toBe(0.5);
    expect(options.ordersCount).toBe(4);
    expect(options.activeMarket).toBe(market);
    expect(options.accountName).toBe('a');
    expect(options.side).toBe(OrderSide.Buy);
    expect(options.reduceOnly).toBe(true);
    expect(options.useChaserOrders).toBe(true);
    expect(options.timeoutPct).toBe(30);
    expect(options.timeoutAction).toBe('next');
    expect(options.failAction).toBe('market');
    expect(options.origin).toBe('cli');
    expect(options.chaserDelay).toBe(1000);
    expect(options.randomnessPercentage).toBe(0);
    expect(options.useLimitOrders).toBe(false);
  });

  it('defaults optional flags to false when absent', () => {
    const options = twapTaskParamsToOptions(makeParams(), ctx);
    expect(options.reduceOnly).toBe(false);
    expect(options.useChaserOrders).toBe(false);
    expect(options.timeoutPct).toBeUndefined();
    expect(options.timeoutAction).toBeUndefined();
    expect(options.failAction).toBeUndefined();
  });
});
