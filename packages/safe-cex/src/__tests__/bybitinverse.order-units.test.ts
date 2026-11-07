import { describe, expect, it, vi } from 'vitest';

import { BYBIT_BRAZIL_SITE_ID } from '../exchanges/bybit/bybit.site';
import { BybitInverseExchange } from './reference/bybit/bybitinverse.exchange';
import { OrderSide, OrderType } from '../types';

const createInverseExchange = () => {
  const store = {
    subscribe: vi.fn(),
    markets: [],
    marketsBySymbol: {},
    tickersBySymbol: {},
    orders: [],
    positions: [],
    addOrUpdateOrders: vi.fn(),
  } as any;
  const exchange = new BybitInverseExchange(
    'bybitinverse' as any,
    { key: 'key', secret: 'secret', testnet: false, bybitSiteId: BYBIT_BRAZIL_SITE_ID } as any,
    store,
  );
  return { exchange: exchange as any, store };
};

// Bravulasco's ladder: $1,000 ETHUSD legs drew as "2,600,192.00 USD" because the
// locally recorded row kept USD contracts in `remaining` and the chart multiplied by price.
describe('bybit inverse order units', () => {
  it('records a locally placed order with remaining in base and USD contracts native', () => {
    const { exchange, store } = createInverseExchange();

    exchange.recordRecentlyPlacedOrderFromOpts(
      'oid-1',
      { symbol: 'ETHUSD', type: OrderType.Limit, side: OrderSide.Buy, amount: 1000, price: 2600 },
      { qty: '1000', price: '2600' },
    );

    const [[[order]]] = store.addOrUpdateOrders.mock.calls;
    expect(order.remaining).toBeCloseTo(1000 / 2600, 8);
    expect(order.extra.remainingNative).toBe(1000);
    expect(order.extra.remainingNotional).toBe(1000);
  });

  it('sizes an amount amend from contracts left after fills', () => {
    const { exchange, store } = createInverseExchange();
    const order = {
      id: 'oid-1',
      symbol: 'ETHUSD',
      price: 2600,
      amount: 1000,
      filled: 400,
      remaining: 600 / 2600,
      extra: { remainingNative: 600 },
    };

    exchange.applyLocalOrderUpdateAfterRestWrite(order, { amount: 1500 });

    const [[[updated]]] = store.addOrUpdateOrders.mock.calls;
    expect(updated.extra.remainingNative).toBe(1100);
    expect(updated.remaining).toBeCloseTo(1100 / 2600, 8);
  });

  it('does not re-convert a web synthetic order whose remaining is already USD', () => {
    const { exchange, store } = createInverseExchange();
    const order = {
      id: 'oid-1',
      symbol: 'ETHUSD',
      price: 2600,
      amount: 600,
      filled: 400,
      remaining: 600,
      extra: { remainingNative: 600 },
    };

    exchange.applyLocalOrderUpdateAfterRestWrite(order, { price: 2400 });

    const [[[updated]]] = store.addOrUpdateOrders.mock.calls;
    expect(updated.remaining).toBeCloseTo(600 / 2400, 8);
    expect(updated.extra.remainingNative).toBe(600);
  });

  it('keeps an amended local order in base units at its new price', () => {
    const { exchange, store } = createInverseExchange();
    const order = {
      id: 'oid-1',
      symbol: 'ETHUSD',
      price: 2600,
      amount: 1000,
      filled: 0,
      remaining: 1000 / 2600,
      extra: { remainingNative: 1000 },
    };

    exchange.applyLocalOrderUpdateAfterRestWrite(order, { price: 2500 });

    const [[[updated]]] = store.addOrUpdateOrders.mock.calls;
    expect(updated.remaining).toBeCloseTo(1000 / 2500, 8);
    expect(updated.extra.remainingNative).toBe(1000);
  });

  it('sizes the row and its split stop-loss and take-profit from contracts left, in coin at each price', () => {
    const { exchange } = createInverseExchange();

    const [row, stopLoss, takeProfit] = exchange.mapOrder({
      orderId: 'oid-1',
      orderStatus: 'New',
      orderType: 'Limit',
      symbol: 'ETHUSD',
      side: 'Buy',
      price: '2600',
      qty: '1000',
      leavesQty: '400',
      cumExecQty: '600',
      stopLoss: '2000',
      takeProfit: '3200',
      timeInForce: 'GTC',
    });

    expect(row.remaining).toBeCloseTo(400 / 2600, 8);
    expect(stopLoss.remaining).toBeCloseTo(400 / 2000, 8);
    expect(stopLoss.extra.remainingNative).toBe(400);
    expect(takeProfit.remaining).toBeCloseTo(400 / 3200, 8);
  });
});
