/**
 * @vitest-environment node
 *
 * Unit tests for the Binance SPOT private WebSocket order handling
 * (`handleOrderEvents`).
 *
 * These feed representative Binance SPOT `executionReport` frames and assert:
 *  (a) a NEW LIMIT_MAKER order is stored with the mapped OrderType (not a
 *      hardcoded Limit).
 *  (b) a NEW STOP_LOSS_LIMIT order maps to OrderType.StopLoss.
 *  (c) a TRADE event with a partial fill updates the stored order
 *      filled/remaining.
 *  (d) a TRADE event with `z === q` transitions the order out of Open (Closed),
 *      while still emitting a `fill` event.
 *
 * Field letters (Binance SPOT executionReport):
 *   o = order type, X = order status, x = execution type, S = side,
 *   c = clientOrderId, s = symbol, p = order price, q = order qty,
 *   z = cumulative filled qty, l = last executed qty.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { BinancePrivateWebsocket } from './binancespot.ws-private';

const makeWs = () => {
  const store = new DefaultStore();
  const emit = vi.fn();
  const parent = {
    store,
    emitter: { emit },
    isDisposed: false,
    options: { testnet: false },
  };
  // The BaseWebSocket constructor only assigns `parent`; no socket is opened
  // until connectAndSubscribe() is called (which we never do here).
  const ws = new BinancePrivateWebsocket(parent as any);
  return { ws, store, emit };
};

// A NEW executionReport frame is nested under `o`.
const newEvent = (over: Record<string, any>) => ({
  o: {
    x: 'NEW',
    X: 'NEW',
    c: 'client-1',
    s: 'BTCUSDT',
    S: 'BUY',
    p: '50000',
    q: '1',
    z: '0',
    ...over,
  },
});

describe('BinanceSpot handleOrderEvents', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('(a) stores a NEW LIMIT_MAKER order with mapped OrderType.Limit, not hardcoded', () => {
    const { ws, store } = makeWs();

    ws.handleOrderEvents([newEvent({ o: 'LIMIT_MAKER', c: 'maker-1' })]);

    const order = store.orders.find((o) => o.id === 'maker-1');
    expect(order).toBeDefined();
    expect(order!.type).toBe(OrderType.Limit);
    expect(order!.status).toBe(OrderStatus.Open);
    expect(order!.side).toBe(OrderSide.Buy);
    expect(order!.reduceOnly).toBe(false);
  });

  it('(b) maps a NEW STOP_LOSS_LIMIT order to OrderType.StopLoss', () => {
    const { ws, store } = makeWs();

    ws.handleOrderEvents([newEvent({ o: 'STOP_LOSS_LIMIT', c: 'stop-1', S: 'SELL' })]);

    const order = store.orders.find((o) => o.id === 'stop-1');
    expect(order).toBeDefined();
    expect(order!.type).toBe(OrderType.StopLoss);
    expect(order!.side).toBe(OrderSide.Sell);
  });

  it('(c) a partial TRADE updates the stored order filled/remaining', () => {
    const { ws, store } = makeWs();

    // First the order is NEW (qty 2, unfilled).
    ws.handleOrderEvents([newEvent({ o: 'LIMIT', c: 'partial-1', q: '2', z: '0' })]);
    expect(store.orders.find((o) => o.id === 'partial-1')!.filled).toBe(0);

    // Then a partial fill: cumulative filled z=0.5 of qty q=2.
    ws.handleOrderEvents([
      {
        o: {
          x: 'TRADE',
          X: 'PARTIALLY_FILLED',
          c: 'partial-1',
          s: 'BTCUSDT',
          S: 'BUY',
          p: '50000',
          q: '2',
          z: '0.5',
          l: '0.5',
        },
      },
    ]);

    const order = store.orders.find((o) => o.id === 'partial-1')!;
    expect(order.filled).toBe(0.5);
    expect(order.remaining).toBe(1.5);
    expect(order.status).toBe(OrderStatus.Open);
  });

  it('(d) a fully-filled TRADE (z === q) closes the order and emits a fill', () => {
    const { ws, store, emit } = makeWs();

    ws.handleOrderEvents([newEvent({ o: 'LIMIT', c: 'fill-1', q: '1', z: '0' })]);

    ws.handleOrderEvents([
      {
        o: {
          x: 'TRADE',
          X: 'FILLED',
          c: 'fill-1',
          s: 'BTCUSDT',
          S: 'BUY',
          p: '50000',
          q: '1',
          z: '1',
          l: '1',
        },
      },
    ]);

    const order = store.orders.find((o) => o.id === 'fill-1')!;
    expect(order.filled).toBe(1);
    expect(order.remaining).toBe(0);
    expect(order.status).toBe(OrderStatus.Closed);

    expect(emit).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({ id: 'fill-1', side: OrderSide.Buy, symbol: 'BTCUSDT', amount: 1 }),
    );
  });
});
