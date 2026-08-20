import type { ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { BybitDemoExchange } from '../../exchanges/bybit/bybitdemo.exchange';
import { DefaultStore } from '../../store/store.base';
import { Order, OrderSide, OrderStatus, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

// Regression: `/v5/order/realtime` lags order creation (worst on api-demo), and
// several paths wholesale-replace `store.orders` from that endpoint —
// private-WS reconnect housekeeping, a startup reinit, the SET_TRADING_STOP
// backstop. A snapshot fetched before the venue indexed a just-placed order
// erased it from the store; the chaser supervisor saw its live order
// "disappear", stopped itself, and cancelled that order on the exchange: the
// "chaser does nothing on Bybit demo" bug.
//
// `preserveRecentlyPlacedOrders` shields an order ONLY between placement and
// its first sighting (REST snapshot / WS frame / our own cancel), bounded by a
// short grace window. A sighted order's later absence (= filled or cancelled)
// is always honored, so the chaser can never re-place on top of a fill.
//
// This file was deleted by SDK-mirror-sync commit 3fd8301610 along with the
// code it guards, which is how the fix silently regressed once already.

const makeOrder = (id: string, overrides: Partial<Order> = {}): Order =>
  ({
    id,
    status: OrderStatus.Open,
    symbol: 'BTCUSDT',
    type: OrderType.Limit,
    side: OrderSide.Buy,
    price: 100,
    amount: 1,
    filled: 0,
    remaining: 1,
    reduceOnly: false,
    lastUpdate: virtualClock.getCurrentTimeMs(),
    ...overrides,
  }) as Order;

const makeExchange = (demo = false) => {
  const store = new DefaultStore();
  const options = { key: 'k', secret: 's' } as ExchangeOptions;
  const exchange = demo
    ? new BybitDemoExchange('bybitdemo' as any, options, store)
    : new BybitExchange('bybit' as any, options, store);
  return { exchange: exchange as any, store };
};

const seedRecentlyPlaced = (exchange: any, order: Order, ageMs = 0) => {
  exchange.recentlyPlacedOrders.set(order.id, {
    placedAt: virtualClock.getCurrentTimeMs() - ageMs,
    order,
  });
};

describe('Bybit recently-placed order snapshot preservation', () => {
  it('re-adds a just-placed order that the REST snapshot omitted', () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);

    const result = exchange.preserveRecentlyPlacedOrders([makeOrder('other')]);

    expect(result.map((o: Order) => o.id)).toEqual(['other', 'new-1']);
    // Still unsighted: the next omitting snapshot must preserve it again.
    expect(exchange.recentlyPlacedOrders.has('new-1')).toBe(true);
  });

  it('prefers the live store row over the constructed placement row', () => {
    const { exchange, store } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1', { price: 100 }));
    const storeRow = makeOrder('new-1', { price: 101, filled: 0.5 });
    store.addOrUpdateOrders([storeRow]);

    const result = exchange.preserveRecentlyPlacedOrders([]);

    expect(result).toHaveLength(1);
    expect(result[0].price).toBe(101);
    expect(result[0].filled).toBe(0.5);
  });

  it('stops protecting an order once a snapshot has sighted it; later absence is honored', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1'));

    const sighting = exchange.preserveRecentlyPlacedOrders([makeOrder('new-1')]);
    expect(sighting.map((o: Order) => o.id)).toEqual(['new-1']);
    expect(exchange.recentlyPlacedOrders.has('new-1')).toBe(false);

    // Order filled/cancelled since: the empty snapshot must stay empty, or the
    // chaser would treat a filled order as resting and place on top of it.
    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('drops protection after the grace window expires', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('stale'), 10_000);

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
    expect(exchange.recentlyPlacedOrders.has('stale')).toBe(false);
  });

  it('markOrderSeen (WS frame / explicit cancel) removes protection immediately', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1'));

    exchange.markOrderSeen('new-1');

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('cancelOrders counts as a sighting so a cancelled order cannot resurrect', async () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);
    // Stub the transport: batch cancel resolves with an OK envelope.
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };

    await exchange.cancelOrders([placed]);

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  // The chaser cancels at the touch, so an order can fill during the cancel
  // round trip. Throwing here made the chaser stop and abandon the chase.
  it('treats already-gone cancel rows as success', async () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);
    exchange.unlimitedXHR = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {},
          retExtInfo: { list: [{ code: 110001, msg: 'order not exists or too late to cancel' }] },
        },
      }),
    };

    await expect(exchange.cancelOrders([placed])).resolves.toBeUndefined();

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('still surfaces genuine cancel failures', async () => {
    const { exchange } = makeExchange();
    exchange.unlimitedXHR = {
      post: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {},
          retExtInfo: { list: [{ code: 10001, msg: 'insufficient permissions' }] },
        },
      }),
    };

    await expect(exchange.cancelOrders([makeOrder('new-1')])).rejects.toThrow('insufficient permissions');
  });

  // The chaser's only fill signal when the Bybit Demo private WS never
  // delivers order frames. Without it the manager cannot distinguish a filled
  // order from a vanished one, so a degraded-WS chase never completes.
  it('fetches fills for chaser order reconciliation by order id', async () => {
    const { exchange, store } = makeExchange();
    store.addOrUpdateOrders([makeOrder('filled-1')]);
    exchange.xhr = {
      get: async () => ({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {
            list: [
              { orderId: 'filled-1', execQty: '0.4' },
              { orderId: 'filled-1', execQty: '0.6' },
              { orderId: 'other', execQty: '5' },
            ],
          },
        },
      }),
    };

    await expect(exchange.fetchFillsForOrderIds(['filled-1'])).resolves.toEqual(new Map([['filled-1', 1]]));
  });

  it('fails closed when Bybit fill reconciliation errors', async () => {
    const { exchange, store } = makeExchange();
    store.addOrUpdateOrders([makeOrder('filled-1')]);
    exchange.xhr = {
      get: async () => ({ data: { retCode: 10001, retMsg: 'bad request', result: { list: [] } } }),
    };

    await expect(exchange.fetchFillsForOrderIds(['filled-1'])).rejects.toThrow('bad request');
  });

  it('recordRecentlyPlacedOrder adds the row to the store on demo only', () => {
    const demo = makeExchange(true);
    demo.exchange.recordRecentlyPlacedOrder(makeOrder('demo-1'));
    expect(demo.store.orders.map((order) => order.id)).toEqual(['demo-1']);

    const live = makeExchange(false);
    live.exchange.recordRecentlyPlacedOrder(makeOrder('live-1'));
    expect(live.store.orders).toEqual([]);
    // Live still gets grace-window preservation against degraded-mode wipes.
    expect(live.exchange.preserveRecentlyPlacedOrders([]).map((o: Order) => o.id)).toEqual(['live-1']);
  });
});
