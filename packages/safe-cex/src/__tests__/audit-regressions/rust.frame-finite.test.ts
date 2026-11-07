import type { Frame } from '@tealstreet/safe-cex-wasm/wire';

import { expect, it } from 'vitest';

import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

const cols = (fields: Record<string, number>) =>
  Object.fromEntries(Object.entries(fields).map(([key, value]) => [key, Float64Array.of(value)]));

// Bybit maps an absent `leverage` to NaN; an engine division can yield ±Infinity.
// Display fields read 0; sizing inputs keep NaN so nuke refuses rather than skips.
const frame = (position: Record<string, number>) =>
  ({
    markets: () => ({
      count: 1,
      flags: Uint32Array.of(1),
      id: () => 'BTCUSDT',
      symbol: () => 'BTCUSDT',
      base: () => 'BTC',
      quote: () => 'USDT',
      ...cols({
        precisionAmount: 0.001,
        precisionPrice: 0.1,
        minAmount: 0.001,
        maxAmount: 100,
        minLeverage: 1,
        maxLeverage: 100,
        minPrice: NaN,
        contractSize: NaN,
        makerFee: NaN,
        takerFee: NaN,
      }),
    }),
    orders: () => ({
      count: 1,
      market: Uint32Array.of(0),
      enums: Uint32Array.of(0),
      id: () => 'one',
      clientOrderId: () => undefined,
      parentId: () => undefined,
      ...cols({
        price: 50000,
        amount: 1,
        filled: 0,
        remaining: 1,
        lastUpdate: 1,
        takeProfit: Infinity,
        stopLoss: -Infinity,
      }),
    }),
    positions: () => ({
      count: 1,
      market: Uint32Array.of(0),
      enums: Uint32Array.of(0),
      id: () => undefined,
      ...cols({
        entryPrice: 40000,
        notional: 1000,
        contracts: 0.025,
        realizedPnl: Infinity,
        marginSize: -Infinity,
        lastUpdate: NaN,
        ...position,
      }),
    }),
    balance: () => undefined,
    tickers: () => undefined,
    tickerDeltas: () => undefined,
    status: () => undefined,
    settings: () => undefined,
  }) as unknown as Frame;

it('reads non-finite display numbers as 0, keeps sizing NaN and drops non-finite optionals', () => {
  const store = new DefaultStore();
  new FrameApplier(store).apply(
    frame({ leverage: NaN, liquidationPrice: Infinity, unrealizedPnl: -Infinity, contracts: NaN }),
  );
  const [position] = store.positions;
  expect(position).toMatchObject({ leverage: 0, liquidationPrice: 0, unrealizedPnl: 0, notional: 1000 });
  expect(position.contracts).toBeNaN();
  expect(position).not.toHaveProperty('realizedPnl');
  expect(position).not.toHaveProperty('marginSize');
  expect(position).not.toHaveProperty('lastUpdate');
  const [order] = store.orders;
  expect(order).not.toHaveProperty('takeProfit');
  expect(order).not.toHaveProperty('stopLoss');
});

it('keeps finite values, including a reported zero', () => {
  const store = new DefaultStore();
  new FrameApplier(store).apply(frame({ leverage: 12.5, liquidationPrice: 0, unrealizedPnl: -3.25 }));
  expect(store.positions[0]).toMatchObject({
    leverage: 12.5,
    liquidationPrice: 0,
    unrealizedPnl: -3.25,
    contracts: 0.025,
  });
});

// Bybit settings copy the position's leverage, and UiStateHandler copies settings back onto positions.
const withSettings = (leverage: number, buy?: number, sell?: number) => {
  const base = frame({ leverage: 5, unrealizedPnl: 0, liquidationPrice: 0 });
  return {
    ...base,
    settings: () => ({
      count: 1,
      market: Uint32Array.of(0),
      leverage: Float64Array.of(leverage),
      ...(buy !== undefined && { buyLeverage: Float64Array.of(buy) }),
      ...(sell !== undefined && { sellLeverage: Float64Array.of(sell) }),
      marginType: () => undefined,
    }),
  } as unknown as Frame;
};

it('omits a symbol whose settings leverage is not finite', () => {
  const store = new DefaultStore();
  new FrameApplier(store).apply(withSettings(NaN));
  expect(store.fetchedLeverage).not.toHaveProperty('BTCUSDT');
  new FrameApplier(store).apply(withSettings(Infinity, 3));
  expect(store.fetchedLeverage).not.toHaveProperty('BTCUSDT');
});

it('keeps finite settings leverage, with a finite side overriding a non-finite total', () => {
  const store = new DefaultStore();
  new FrameApplier(store).apply(withSettings(NaN, 3, 4));
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 3, sellLeverage: 4 });
  new FrameApplier(store).apply(withSettings(10, NaN));
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 10, sellLeverage: 10 });
});
