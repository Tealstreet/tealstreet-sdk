import { describe, expect, it, vi } from 'vitest';

import { HyperliquidPrivateWebSocket } from './reference/hyperliquid/hyperliquid.ws-private';
import { OrderSide, OrderStatus, OrderType } from '../types';

vi.mock('./reference/hyperliquid/hyperliquid.cross-cache', () => ({
  hasSeenFill: vi.fn(),
  markFillSeen: vi.fn(),
}));

// Hyperliquid's `orderUpdates` frames never carry `orderType` / `triggerPx` /
// `isTrigger`; those arrive on REST `frontendOpenOrders`. A reduce-only frame
// without them must therefore be deferred to REST rather than inserted as a
// Limit at `limitPx` — for a TP/SL attached to a limit entry that `limitPx`
// is the entry price, which drew the stop on the break-even line.
const createHarness = (orders: any[] = []) => {
  const market = { id: 'BTC', symbol: 'BTC-PERP', info: {} };
  const store = {
    marketsById: { BTC: market },
    marketsBySymbol: { 'BTC-PERP': market },
    orders,
    positions: [],
    addOrUpdateOrder: vi.fn(),
    removeOrder: vi.fn(),
  };
  const parent = {
    name: 'hyperliquid',
    isDisposed: false,
    options: { testnet: false },
    store,
    hip3DexNames: [],
    marketsLowerCaseMap: new Map(),
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    isOrderCancelled: vi.fn(() => false),
    scheduleOrderRefresh: vi.fn(),
    getPendingHip3Order: vi.fn(),
    matchPrePlacement: vi.fn(),
    matchPrePlacementTrigger: vi.fn(),
    getPlacedLimitOrderTimeInForce: vi.fn(),
    suppressCancelledOrder: vi.fn(),
    forgetLimitOrderTimeInForce: vi.fn(),
  };
  const ws = new HyperliquidPrivateWebSocket(parent as any);
  return { ws, store, parent };
};

const frame = (overrides: Record<string, unknown>) => ({
  status: 'open',
  order: {
    coin: 'BTC',
    side: 'A',
    limitPx: '100000',
    sz: '0.5',
    origSz: '0.5',
    oid: 42,
    timestamp: 1_700_000_000_000,
    reduceOnly: true,
    ...overrides,
  },
});

describe('hyperliquid orderUpdates: core-perp reduce-only frames without trigger fields', () => {
  it('defers a SIZED reduce-only frame (attached or partial TP/SL) to REST instead of inserting a Limit', () => {
    const { ws, store, parent } = createHarness();
    ws.handleOrderUpdates([frame({})]);
    expect(store.addOrUpdateOrder).not.toHaveBeenCalled();
    expect(parent.scheduleOrderRefresh).toHaveBeenCalledTimes(1);
  });

  it('still defers the size-0 positional TP/SL frame', () => {
    const { ws, store, parent } = createHarness();
    ws.handleOrderUpdates([frame({ sz: '0', origSz: '0' })]);
    expect(store.addOrUpdateOrder).not.toHaveBeenCalled();
    expect(parent.scheduleOrderRefresh).toHaveBeenCalledTimes(1);
  });

  it('preserves type and trigger price once REST has put the order in the store', () => {
    const existing = {
      id: '42',
      symbol: 'BTC-PERP',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 95000,
      amount: 0.5,
      filled: 0,
      remaining: 0.5,
      reduceOnly: true,
      status: OrderStatus.Open,
    };
    const { ws, store, parent } = createHarness([existing]);
    ws.handleOrderUpdates([frame({})]);
    expect(parent.scheduleOrderRefresh).not.toHaveBeenCalled();
    expect(store.addOrUpdateOrder).toHaveBeenCalledTimes(1);
    const inserted = store.addOrUpdateOrder.mock.calls[0][0];
    expect(inserted.type).toBe(OrderType.StopLoss);
    expect(inserted.price).toBe(95000);
  });

  it('inserts a non-reduce-only frame as a Limit at limitPx as before', () => {
    const { ws, store } = createHarness();
    ws.handleOrderUpdates([frame({ reduceOnly: false })]);
    // (A refresh may still be scheduled for the missing `tif`; that is the
    // separate time-in-force fallback, not the deferral under test.)
    const inserted = store.addOrUpdateOrder.mock.calls[0][0];
    expect(inserted.type).toBe(OrderType.Limit);
    expect(inserted.price).toBe(100000);
  });

  it('uses the cached trigger for a core-perp STOP ENTRY (non-reduce-only) instead of a Limit at the slippage cap', () => {
    const { ws, store, parent } = createHarness();
    parent.matchPrePlacementTrigger.mockReturnValueOnce({ type: OrderType.StopLoss, price: 105000 });
    ws.handleOrderUpdates([frame({ reduceOnly: false, side: 'B', limitPx: '110250' })]);
    expect(parent.matchPrePlacementTrigger).toHaveBeenCalledWith('BTC', OrderSide.Buy, 0.5);
    const inserted = store.addOrUpdateOrder.mock.calls[0][0];
    expect(inserted.type).toBe(OrderType.StopLoss);
    expect(inserted.price).toBe(105000);
    expect(inserted.reduceOnly).toBe(false);
  });

  it('keeps a stored stop entry\'s type and trigger price on a later frame without trigger fields', () => {
    const existing = {
      id: '42',
      symbol: 'BTC-PERP',
      type: OrderType.StopLoss,
      side: OrderSide.Buy,
      price: 105000,
      amount: 0.5,
      filled: 0,
      remaining: 0.5,
      reduceOnly: false,
      status: OrderStatus.Open,
    };
    const { ws, store, parent } = createHarness([existing]);
    ws.handleOrderUpdates([frame({ reduceOnly: false, side: 'B', limitPx: '110250' })]);
    expect(parent.matchPrePlacementTrigger).not.toHaveBeenCalled();
    const inserted = store.addOrUpdateOrder.mock.calls[0][0];
    expect(inserted.type).toBe(OrderType.StopLoss);
    expect(inserted.price).toBe(105000);
  });
});
