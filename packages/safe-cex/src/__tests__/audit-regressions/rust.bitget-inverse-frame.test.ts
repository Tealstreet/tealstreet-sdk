import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { expect, it } from 'vitest';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

const cols = (fields: Record<string, number>) => Object.fromEntries(
  Object.entries(fields).map(([key, value]) => [key, Float64Array.of(value)]),
);

it('preserves inverse native quantities and collateral after worker frame hydration', () => {
  const coins = { BTC: { base: 'BTC', amount: 0.1, notional: 5000 } };
  const frame = {
    // Active + inverse, with inverseNative (2) quantity units in bits 8..12.
    markets: () => ({ count: 1, flags: Uint32Array.of(3 | (2 << 8)), id: () => 'BTCUSD', symbol: () => 'BTCUSD',
      base: () => 'BTC', quote: () => 'USD',
      ...cols({ precisionAmount: 1, precisionPrice: 0.1, minAmount: 1, maxAmount: 2000,
        minLeverage: 1, maxLeverage: 125, minPrice: NaN, contractSize: NaN, makerFee: NaN, takerFee: NaN }),
    }),
    orders: () => ({ count: 1, market: Uint32Array.of(0), enums: Uint32Array.of(0),
      id: () => 'one', clientOrderId: () => undefined, parentId: () => undefined,
      ...cols({ price: 50000, amount: 1000, filled: 100, remaining: 0.018, lastUpdate: 1, takeProfit: NaN, stopLoss: NaN }),
    }),
    positions: () => ({ count: 1, market: Uint32Array.of(0), enums: Uint32Array.of(0),
      id: () => undefined,
      ...cols({ entryPrice: 40000, notional: 1000, leverage: 10, unrealizedPnl: 0.002, realizedPnl: 0,
        contracts: 0.02, liquidationPrice: 20000, marginSize: 0.01, lastUpdate: 1 }),
    }),
    balance: () => ({ total: 4900, used: 1000, free: 4000, upnl: 100, coins }),
    tickers: () => undefined, tickerDeltas: () => undefined, status: () => undefined, settings: () => undefined,
  } as unknown as Frame;
  const store = new DefaultStore();
  new FrameApplier(store).apply(frame);
  expect(store.markets[0].isInverse).toBe(true);
  expect(store.orders[0]).toMatchObject({ amount: 1000, filled: 100, remaining: 0.018, extra: { remainingNative: 900 } });
  expect(store.positions[0]).toMatchObject({ id: 'BTCUSD', contracts: 0.02, notional: 1000,
    extra: { contractsNative: 1000, unrealizedPnlNative: 0.002 } });
  expect(store.balance.coins).toEqual(coins);
  const originalOrders = frame.orders();
  frame.orders = () => ({ ...originalOrders!, remainingNative: Float64Array.of(777) });
  new FrameApplier(store).apply(frame);
  expect(store.orders[0].extra).toEqual({ remainingNative: 777, remainingNotional: 777 });
  const originalBalance = frame.balance()!;
  frame.balance = () => ({ total: 5000, used: 1000, free: 4000, upnl: 0 });
  new FrameApplier(store).apply(frame);
  expect(store.balance.coins).toEqual(coins);
  frame.balance = () => ({ ...originalBalance, coins: {} });
  new FrameApplier(store).apply(frame);
  expect(store.balance.coins).toEqual({});
  frame.orders = () => ({ ...originalOrders!, enums: Uint32Array.of((5 << 8) | (2 << 28)) });
  new FrameApplier(store).apply(frame);
  expect(store.orders[0].slTriggerType).toBe('last_price');
});
