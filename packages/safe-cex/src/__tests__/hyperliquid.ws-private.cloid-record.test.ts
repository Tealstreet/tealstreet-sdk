import { describe, expect, it, vi } from 'vitest';

import { HyperliquidPrivateWebSocket } from './reference/hyperliquid/hyperliquid.ws-private';
import { OrderSide, OrderTimeInForce, OrderType } from '../types';

vi.mock('./reference/hyperliquid/hyperliquid.cross-cache', () => ({
  hasSeenFill: vi.fn(),
  markFillSeen: vi.fn(),
}));

// Every order we place carries a cloid, recorded before the REST call with
// what we signed. A frame that echoes a recorded cloid is built from the
// record — no type/trigger-price inference, no deferral to REST. A frame with
// no cloid, or one we have no record of (another client's order), keeps
// today's inference chain.
const CLOID = '0x0123456789abcdef0123456789abcdef';

const createHarness = (records: Record<string, unknown> = {}) => {
  const core = { id: 'BTC', symbol: 'BTC-PERP', info: {} };
  const hip3 = { id: 'xyz:TSLA', symbol: 'XYZ|TSLA-PERP', info: { hip3Dex: 'xyz' } };
  const store = {
    marketsById: { BTC: core, 'xyz:TSLA': hip3 },
    marketsBySymbol: { 'BTC-PERP': core, 'XYZ|TSLA-PERP': hip3 },
    orders: [],
    positions: [],
    addOrUpdateOrder: vi.fn(),
    removeOrder: vi.fn(),
  };
  const parent = {
    name: 'hyperliquid',
    isDisposed: false,
    options: { testnet: false },
    store,
    hip3DexNames: ['xyz'],
    marketsLowerCaseMap: new Map(),
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    isOrderCancelled: vi.fn(() => false),
    scheduleOrderRefresh: vi.fn(),
    getPendingHip3Order: vi.fn(),
    removePendingHip3Order: vi.fn(),
    matchPrePlacement: vi.fn(),
    matchPrePlacementTrigger: vi.fn(),
    getPlacedLimitOrderTimeInForce: vi.fn(),
    suppressCancelledOrder: vi.fn(),
    forgetLimitOrderTimeInForce: vi.fn(),
    getPlacementRecord: vi.fn((cloid: string) => records[cloid]),
    forgetPlacementRecord: vi.fn(),
    forgetPrePlacement: vi.fn(),
  };
  const ws = new HyperliquidPrivateWebSocket(parent as any);
  return { ws, store, parent };
};

// A live frame's shape (2026-10-11): no orderType / triggerPx / tif, and no
// `reduceOnly` key at all on an order that is not reduce-only.
const frame = (order: Record<string, unknown>, status = 'open') => ({
  status,
  order: {
    coin: 'BTC',
    side: 'A',
    limitPx: '100000',
    sz: '0.5',
    origSz: '0.5',
    oid: 42,
    timestamp: 1_700_000_000_000,
    cloid: CLOID,
    ...order,
  },
});

const record = (overrides: Record<string, unknown>) => ({
  type: OrderType.StopLoss,
  price: 95000,
  side: OrderSide.Sell,
  amount: 0.5,
  reduceOnly: true,
  symbol: 'BTC-PERP',
  placedAt: Date.now(),
  ...overrides,
});

const expectNoHeuristics = (parent: ReturnType<typeof createHarness>['parent']) => {
  expect(parent.scheduleOrderRefresh).not.toHaveBeenCalled();
  expect(parent.getPendingHip3Order).not.toHaveBeenCalled();
  expect(parent.matchPrePlacement).not.toHaveBeenCalled();
  expect(parent.matchPrePlacementTrigger).not.toHaveBeenCalled();
  expect(parent.getPlacedLimitOrderTimeInForce).not.toHaveBeenCalled();
};

describe('hyperliquid orderUpdates: a recorded cloid identifies the order exactly', () => {
  it('inserts a sized reduce-only stop (an attached leg) at its trigger price instead of deferring it', () => {
    const { ws, store, parent } = createHarness({ [CLOID]: record({}) });
    ws.handleOrderUpdates([frame({ reduceOnly: true })]);

    expectNoHeuristics(parent);
    expect(store.addOrUpdateOrder).toHaveBeenCalledTimes(1);
    expect(store.addOrUpdateOrder.mock.calls[0][0]).toMatchObject({
      id: '42',
      type: OrderType.StopLoss,
      price: 95000,
      reduceOnly: true,
      timeInForce: undefined,
      amount: 0.5,
      remaining: 0.5,
      clientOrderId: CLOID,
    });
    expect(parent.forgetPrePlacement).toHaveBeenCalledWith('BTC', OrderSide.Sell, CLOID);
  });

  it('inserts a positional TP/SL (size 0) from its record', () => {
    const { ws, store, parent } = createHarness({
      [CLOID]: record({ type: OrderType.TakeProfit, price: 110000, amount: 0 }),
    });
    ws.handleOrderUpdates([frame({ reduceOnly: true, sz: '0.0', origSz: '0.0' })]);

    expectNoHeuristics(parent);
    expect(store.addOrUpdateOrder.mock.calls[0][0]).toMatchObject({ type: OrderType.TakeProfit, price: 110000 });
  });

  it('takes reduce-only and time-in-force from the record when the frame carries neither', () => {
    const { ws, store, parent } = createHarness({
      [CLOID]: record({
        type: OrderType.Limit,
        price: 100000,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.PostOnly,
      }),
    });
    ws.handleOrderUpdates([frame({ side: 'B' })]);

    expectNoHeuristics(parent);
    expect(store.addOrUpdateOrder.mock.calls[0][0]).toMatchObject({
      type: OrderType.Limit,
      price: 100000,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.PostOnly,
      clientOrderId: CLOID,
    });
  });

  it('recovers a core-perp stop entry without the strict pre-placement match', () => {
    const { ws, store, parent } = createHarness({
      [CLOID]: record({ side: OrderSide.Buy, price: 105000, reduceOnly: false }),
    });
    ws.handleOrderUpdates([frame({ side: 'B', limitPx: '110250' })]);

    expectNoHeuristics(parent);
    expect(store.addOrUpdateOrder.mock.calls[0][0]).toMatchObject({
      type: OrderType.StopLoss,
      price: 105000,
      reduceOnly: false,
    });
  });

  it('skips the HIP-3 pending and pre-placement caches', () => {
    const { ws, store, parent } = createHarness({
      [CLOID]: record({ type: OrderType.Limit, price: 400, reduceOnly: false, symbol: 'XYZ|TSLA-PERP' }),
    });
    ws.handleOrderUpdates([frame({ coin: 'xyz:TSLA', side: 'B', limitPx: '400' })]);

    expectNoHeuristics(parent);
    expect(store.addOrUpdateOrder.mock.calls[0][0]).toMatchObject({
      symbol: 'XYZ|TSLA-PERP',
      type: OrderType.Limit,
      price: 400,
      status: 'open',
    });
  });

  it('drops the record when the order fills or is cancelled', () => {
    const { ws, store, parent } = createHarness({ [CLOID]: record({}) });
    ws.handleOrderUpdates([frame({}, 'filled'), frame({ oid: 43, cloid: '0x' + 'a'.repeat(32) }, 'canceled')]);

    expect(store.removeOrder).toHaveBeenCalledTimes(2);
    expect(parent.forgetPlacementRecord.mock.calls).toEqual([[CLOID], ['0x' + 'a'.repeat(32)]]);
  });
});

describe('hyperliquid orderUpdates: frames without a record keep the fallback chain', () => {
  it('defers an unknown-cloid reduce-only frame to REST, as before', () => {
    const { ws, store, parent } = createHarness();
    ws.handleOrderUpdates([frame({ reduceOnly: true })]);

    expect(parent.getPlacementRecord).toHaveBeenCalledWith(CLOID);
    expect(store.addOrUpdateOrder).not.toHaveBeenCalled();
    expect(parent.scheduleOrderRefresh).toHaveBeenCalledTimes(1);
  });

  it('still consults the strict stop-entry match, and stamps the cloid it saw', () => {
    const { ws, store, parent } = createHarness();
    parent.matchPrePlacementTrigger.mockReturnValueOnce({ type: OrderType.StopLoss, price: 105000 });
    ws.handleOrderUpdates([frame({ side: 'B', limitPx: '110250' })]);

    expect(parent.matchPrePlacementTrigger).toHaveBeenCalledWith('BTC', OrderSide.Buy, 0.5);
    expect(store.addOrUpdateOrder.mock.calls[0][0]).toMatchObject({
      type: OrderType.StopLoss,
      price: 105000,
      clientOrderId: CLOID,
    });
  });

  it('leaves clientOrderId off an order whose frame carries no cloid', () => {
    const { ws, store, parent } = createHarness();
    ws.handleOrderUpdates([frame({ cloid: undefined })]);

    expect(parent.getPlacementRecord).not.toHaveBeenCalled();
    expect(store.addOrUpdateOrder.mock.calls[0][0]).not.toHaveProperty('clientOrderId');
  });
});
