import type { Frame } from '@tealstreet/safe-cex-wasm/wire';

import assert from 'node:assert/strict';

import { expect, it } from 'vitest';

import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { inverseUnitsFrame } from '../fixtures/scxfFrame';

const cols = (fields: Record<string, number | null>) =>
  Object.fromEntries(Object.entries(fields).map(([key, value]) => [key, [value]]));

// Bybit maps an absent `leverage` to NaN; an engine division can yield ±Infinity.
// Display fields read 0; sizing inputs keep NaN so nuke refuses rather than skips.
const frame = (position: Record<string, number | null>) =>
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
        takeProfit: NaN,
        stopLoss: NaN,
      }),
    }),
    positions: () => ({
      count: 1,
      market: Uint32Array.of(0),
      enums: Uint32Array.of(0),
      id: () => 'BTCUSDT',
      symbol: () => 'BTCUSDT',
      realizedPnl: [undefined],
      marginSize: [undefined],
      lastUpdate: [undefined],
      contractsNative: [undefined],
      unrealizedPnlNative: [undefined],
      ...cols({
        entryPrice: 40000,
        notional: 1000,
        contracts: 0.025,
        ...position,
      }),
    }),
    balance: () => undefined,
    tickers: () => undefined,
    tickerDeltas: () => undefined,
    tickerObservations: () => undefined,
    status: () => undefined,
    settings: () => undefined,
  }) as unknown as Frame;

it('preserves native normalized fields and sizing guards without repairing them', () => {
  const store = new DefaultStore();
  new FrameApplier(store).apply(frame({ leverage: 0, liquidationPrice: null, unrealizedPnl: 0, contracts: NaN }));
  const [position] = store.positions;
  expect(position).toMatchObject({ leverage: 0, liquidationPrice: null, unrealizedPnl: 0, notional: 1000 });
  expect(position.contracts).toBeNaN();
  expect(position).not.toHaveProperty('realizedPnl');
  expect(position).not.toHaveProperty('marginSize');
  expect(position).not.toHaveProperty('lastUpdate');
  const [order] = store.orders;
  expect(order).not.toHaveProperty('takeProfit');
  expect(order).not.toHaveProperty('stopLoss');
});

it('refuses non-finite Order fields rather than repairing them in the frame reader', () => {
  const input = frame({ leverage: 5, unrealizedPnl: 0, liquidationPrice: null });
  const orders = input.orders();
  if (orders === undefined) throw new Error('Expected Order fixture');
  orders.takeProfit[0] = Infinity;
  input.orders = () => orders;
  expect(() => new FrameApplier(new DefaultStore()).apply(input)).toThrow('invalid Order optional number');
});

it('keeps positive liquidation prices and other finite values', () => {
  const store = new DefaultStore();
  new FrameApplier(store).apply(frame({ leverage: 12.5, liquidationPrice: 67.8, unrealizedPnl: -3.25 }));
  expect(store.positions[0]).toMatchObject({
    leverage: 12.5,
    liquidationPrice: 67.8,
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

it('refuses absent or non-finite native rails without replacing confirmed leverage', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  applier.apply(withSettings(5, 3, 4));
  const previous = store.fetchedLeverage;
  expect(() => applier.apply(withSettings(NaN))).toThrow('Native Store leverage rails are required');
  expect(() => applier.apply(withSettings(Infinity, 3, Infinity))).toThrow('Invalid native Store leverage rails');
  expect(store.fetchedLeverage).toBe(previous);
});

it('preserves finite native rails independently of the legacy scalar, including zero', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  applier.apply(withSettings(NaN, 3, 4));
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 3, sellLeverage: 4 });
  applier.apply(withSettings(10, 10, 10));
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 10, sellLeverage: 10 });
  applier.apply(withSettings(NaN, 0, 0));
  expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 0, sellLeverage: 0 });
});

it.each([NaN, Infinity, -Infinity, 0, -0, -1])(
  'exposes an unusable native liquidation price as null (%s)',
  (liquidationPrice) => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(frame({ leverage: 5, liquidationPrice: 67.8, unrealizedPnl: 0 }));
    expect(store.positions[0].liquidationPrice).toBe(67.8);
    applier.apply(frame({ leverage: 5, liquidationPrice: null, unrealizedPnl: 0 }));
    expect(store.positions[0].liquidationPrice).toBeNull();
    expect(JSON.parse(JSON.stringify(store.positions[0])).liquidationPrice).toBeNull();
  },
);

it.each([NaN, Infinity, -Infinity, 0, -0, -1])(
  'preserves unknown liquidation through packed frames and serialization (%s)',
  (liquidationPrice) => {
    const packed = inverseUnitsFrame({ positions: true, inverse: false, positionLiquidationPrice: liquidationPrice });
    const positions = packed.positions();
    assert.ok(positions, 'Packed frame must contain a positions section');
    expect(positions.liquidationPrice[0]).toBeNull();
    const store = new DefaultStore();
    new FrameApplier(store).apply(packed);
    expect(store.positions[0].liquidationPrice).toBeNull();
    expect(JSON.parse(JSON.stringify(store.serialize())).positions[0].liquidationPrice).toBeNull();
  },
);
