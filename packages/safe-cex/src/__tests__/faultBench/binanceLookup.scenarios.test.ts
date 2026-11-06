import type { Market } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderStatus } from '../../types';
import { createFakeVenue } from './fakeVenue';
import { createFaultAdapter } from './faultAdapter';

const MARKET: Market = {
  // Binance's real shape: the id is NOT what orders are placed with.
  id: 'BTC/USDT:USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  info: {},
};

const exchanges: BinanceExchange[] = [];

// Binance's query-order endpoint, by client ID: live or closed, -2013 for an ID it never saw.
const setup = (rows: Record<string, Record<string, unknown>>) => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new BinanceExchange(
    'binance',
    { key: 'key', secret: 'secret', tealstreetTunnel: false } as any,
    store,
  );
  exchanges.push(exchange);
  const errors: string[] = [];
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation((error) => {
    errors.push(String(error));
  });
  const venue = createFakeVenue({
    'get /fapi/v1/order': ({ params }) => {
      const { symbol, origClientOrderId } = params as { symbol: string; origClientOrderId: string };
      if (symbol !== 'BTCUSDT') return { status: 400, data: { code: -1121, msg: 'Invalid symbol.' } };
      const row = rows[origClientOrderId];
      return row ? { status: 200, data: row } : { status: 400, data: { code: -2013, msg: 'Order does not exist.' } };
    },
  });
  const bench = createFaultAdapter(venue);
  bench.install(((exchange as any)._primaryXhr ?? (exchange as any).xhr) as any);
  return { exchange, venue, errors };
};

afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  vi.restoreAllMocks();
});

describe('bad link: finding a Binance order whose response was lost', () => {
  it('finds an order that already filled, with its real status', async () => {
    const { exchange } = setup({
      'x-s3vFFRQpabc': {
        orderId: 42,
        clientOrderId: 'x-s3vFFRQpabc',
        symbol: 'BTCUSDT',
        side: 'BUY',
        type: 'LIMIT',
        status: 'FILLED',
        price: '50000',
        origQty: '1',
        executedQty: '1',
        time: 1_000_000,
        updateTime: 1_000_500,
      },
    });

    await expect(exchange.findOrderByClientOrderId('BTCUSDT', 'x-s3vFFRQpabc')).resolves.toMatchObject({
      found: true,
      order: { id: '42', status: OrderStatus.Closed, filled: 1, remaining: 0 },
    });
  });

  it('reports an ID Binance never saw as definitively not found, without a user-facing error', async () => {
    const { exchange, errors } = setup({});

    await expect(exchange.findOrderByClientOrderId('BTCUSDT', 'x-s3vFFRQpnone')).resolves.toEqual({
      found: false,
      definitive: true,
    });
    expect(errors).toEqual([]);
  });
});
