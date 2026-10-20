import type { Frame } from '@tealstreet/safe-cex-wasm/wire';

import { describe, expect, it } from 'vitest';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

const numbers = <Key extends string>(values: Record<Key, number>) => Object.fromEntries(
  Object.entries<number>(values).map(([key, value]) => [key, Float64Array.of(value)]),
) as Record<Key, Float64Array>;

// Frame readers expose typed columns; quantities here are published base units.
const frame = () => ({
  markets: () => ({
    count: 1, id: () => 'BTC_USDT', symbol: () => 'BTCUSDT', base: () => 'BTC', quote: () => 'USDT',
    flags: Uint32Array.of(1),
    ...numbers({ precisionAmount: 0.0001, precisionPrice: 0.1, minAmount: 0.0001, maxAmount: 40,
      minLeverage: 1, maxLeverage: 500, minPrice: NaN, contractSize: 0.0001, makerFee: 0, takerFee: 0.0002 }),
  }),
  orders: () => ({ count: 1, id: () => 'one', clientOrderId: () => undefined, parentId: () => undefined,
    market: Uint32Array.of(0), enums: Uint32Array.of(0),
    ...numbers({ price: 60000, amount: 0.006, filled: 0.002, remaining: 0.004, lastUpdate: 1, takeProfit: NaN, stopLoss: NaN }),
  }),
  tickers: () => undefined, tickerDeltas: () => undefined, positions: () => undefined,
  balance: () => undefined, status: () => undefined, settings: () => undefined,
}) as unknown as Frame;

describe('native placement quantity bridge', () => {
  it('reconstructs MEXC chaser quantities from base-sized frame columns', () => {
    const store = new DefaultStore();
    new FrameApplier(store, true).apply(frame());
    expect(store.orders[0].extra).toMatchObject({ remainingNative: 40, filledInPlacementUnits: 20 });
    expect(store.orders[0].amount).toBe(0.006);
  });

  it('preserves exact MEXC native leaves instead of reconstructing a fractional contract', () => {
    const value = frame();
    const order = value.orders()!;
    value.orders = () => ({
      ...order,
      amount: Float64Array.of(4 * 0.0001),
      filled: Float64Array.of(3 * 0.0001),
      remaining: Float64Array.of(4 * 0.0001 - 3 * 0.0001),
      remainingNative: Float64Array.of(1),
    });
    const store = new DefaultStore();
    new FrameApplier(store, true).apply(value);
    expect(store.orders[0].extra?.remainingNative).toBe(1);
  });

  it.each([false, true])('restores exact decimal lot fills and position sizes (native tail=%s)', (nativeTail) => {
    const value = frame();
    const market = value.markets()!;
    value.markets = () => ({ ...market, contractSize: Float64Array.of(0.1) });
    const order = value.orders()!;
    value.orders = () => ({
      ...order,
      amount: Float64Array.of(0.7), filled: Float64Array.of(0.3), remaining: Float64Array.of(0.4),
      ...(nativeTail && { remainingNative: Float64Array.of(5) }),
    });
    value.positions = () => ({
      count: 1, market: Uint32Array.of(0), enums: Uint32Array.of(0), id: () => undefined,
      ...numbers({ entryPrice: 100, notional: 30, leverage: 10, unrealizedPnl: 0.2, realizedPnl: 0,
        contracts: 0.3, liquidationPrice: 50, marginSize: 3, lastUpdate: 1 }),
    });
    const store = new DefaultStore();
    new FrameApplier(store, true).apply(value);
    expect(store.orders[0]).toMatchObject({ amount: 0.7, filled: 0.3, remaining: 0.4,
      extra: { remainingNative: nativeTail ? 5 : 4, filledInPlacementUnits: 3 } });
    expect(store.positions[0]).toMatchObject({ contracts: 0.3, extra: { contractsNative: 3 } });
  });

  it('keeps base placement units for other venues', () => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(frame());
    expect(store.orders[0].extra).toBeUndefined();
  });
});

it('preserves separate hedge leverage rails in the store metadata', () => {
  const value = frame();
  value.settings = () => ({ count: 1, market: Uint32Array.of(0), leverage: Float64Array.of(20),
    buyLeverage: Float64Array.of(10), sellLeverage: Float64Array.of(15), marginType: () => 'cross' });
  const store = new DefaultStore();
  new FrameApplier(store, true).apply(value);
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 10, sellLeverage: 15 });
});
