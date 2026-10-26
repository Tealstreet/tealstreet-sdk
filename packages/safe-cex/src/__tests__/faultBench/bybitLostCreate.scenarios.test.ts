import type { Market } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { createFakeVenue } from './fakeVenue';
import { createFaultAdapter } from './faultAdapter';

// Bybit used to swallow a create whose response was lost and return `[]`, which a
// chase read as "not placed" and placed again on top of the live order.

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  info: {},
};

const CREATE_ORDER = 'post /v5/order/create';

const exchanges: BybitExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  vi.restoreAllMocks();
});

const setup = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new BybitExchange('bybit' as any, { key: 'k', secret: 's' } as any, store);
  exchanges.push(exchange);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  const venue = createFakeVenue({
    [CREATE_ORDER]: () => ({ status: 200, data: { retCode: 0, retMsg: 'OK', result: { orderId: 'bybit-1' } } }),
    'get /v5/position/list': () => ({ status: 200, data: { retCode: 0, retMsg: 'OK', result: { list: [] } } }),
  });
  const bench = createFaultAdapter(venue);
  bench.install((exchange as any).xhr);
  return { exchange, venue, bench };
};

const limit = { symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, price: 50_000, amount: 1 };

describe('Bybit lost create', () => {
  it('throws for a single order whose response was lost, so the caller looks it up', async () => {
    const { exchange, venue, bench } = setup();
    bench.on(CREATE_ORDER, 'loseResponse');

    await expect(exchange.placeOrder({ ...limit, clientOrderId: 'cid-1' })).rejects.toMatchObject({
      code: 'ECONNRESET',
    });
    expect(venue.received.filter((r: any) => r.key === CREATE_ORDER)).toHaveLength(1);
  });

  // Stops, conditionals and scaled legs carry no caller id, so there is nothing to look up.
  it('keeps returning [] for an order with no caller client id', async () => {
    const { exchange, bench } = setup();
    bench.on(CREATE_ORDER, 'loseResponse');

    await expect(exchange.placeOrder(limit)).resolves.toEqual([]);
  });

  it('still returns [] for a refused order, which is not ambiguous', async () => {
    const { exchange, bench } = setup();
    bench.on(CREATE_ORDER, { reject: 400 } as any);

    await expect(exchange.placeOrder({ ...limit, clientOrderId: 'cid-2' })).resolves.toEqual([]);
  });
});
