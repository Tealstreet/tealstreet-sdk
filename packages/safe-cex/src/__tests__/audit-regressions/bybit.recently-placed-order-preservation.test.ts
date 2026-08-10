import type { ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { BybitDemoExchange } from '../../exchanges/bybit/bybitdemo.exchange';
import { DefaultStore } from '../../store/store.base';
import { Order, OrderSide, OrderStatus, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

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

describe('Bybit recently placed order preservation', () => {
  it('re-adds a just-placed order that the REST snapshot omitted', () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);

    const result = exchange.preserveRecentlyPlacedOrders([makeOrder('other')]);

    expect(result.map((order: Order) => order.id)).toEqual(['other', 'new-1']);
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

  it('stops protecting an order once a snapshot has sighted it', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1'));

    const sighting = exchange.preserveRecentlyPlacedOrders([makeOrder('new-1')]);
    expect(sighting.map((order: Order) => order.id)).toEqual(['new-1']);
    expect(exchange.recentlyPlacedOrders.has('new-1')).toBe(false);
    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('drops protection after the grace window expires', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('stale'), 10_000);

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
    expect(exchange.recentlyPlacedOrders.has('stale')).toBe(false);
  });

  it('markOrderSeen removes protection immediately', () => {
    const { exchange } = makeExchange();
    seedRecentlyPlaced(exchange, makeOrder('new-1'));

    exchange.markOrderSeen('new-1');

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('cancelOrders counts as a sighting so a cancelled order cannot resurrect', async () => {
    const { exchange } = makeExchange();
    const placed = makeOrder('new-1');
    seedRecentlyPlaced(exchange, placed);
    exchange.unlimitedXHR = {
      post: async () => ({ data: { retCode: 0, retMsg: 'OK', result: {}, retExtInfo: { list: [] } } }),
    };

    await exchange.cancelOrders([placed]);

    expect(exchange.preserveRecentlyPlacedOrders([])).toEqual([]);
  });

  it('recordRecentlyPlacedOrder adds the row to the store on demo only', () => {
    const demo = makeExchange(true);
    demo.exchange.recordRecentlyPlacedOrder(makeOrder('demo-1'));
    expect(demo.store.orders.map((order) => order.id)).toEqual(['demo-1']);

    const live = makeExchange(false);
    live.exchange.recordRecentlyPlacedOrder(makeOrder('live-1'));
    expect(live.store.orders).toEqual([]);
    expect(live.exchange.preserveRecentlyPlacedOrders([]).map((order: Order) => order.id)).toEqual(['live-1']);
  });
});
