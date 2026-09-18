import type { Axios } from 'axios';
import type { LIST_ORDERS_RESPONSE } from '../../exchanges/coinbase/coinbase.types';

import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../../exchanges/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../../exchanges/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, Market, OrderSide, OrderStatus, OrderType } from '../../types';

const OPEN_ORDER_STATUSES = ['PENDING', 'OPEN', 'QUEUED', 'CANCEL_QUEUED', 'EDIT_QUEUED'];

const SPOT_MARKET: Market = {
  id: 'BTC-USD',
  symbol: 'BTC-USD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  isSpot: true,
  isExpiring: false,
  precision: { amount: 0.00000001, price: 0.01 },
  limits: {
    amount: { min: 0.00000001, max: 1000 },
    leverage: { min: 1, max: 1 },
  },
  contractSize: 1,
  info: { product_type: 'SPOT' },
};

const makeRestOrder = (
  overrides: Partial<LIST_ORDERS_RESPONSE['orders'][number]> = {},
): LIST_ORDERS_RESPONSE['orders'][number] => ({
  order_id: 'coinbase-order-id',
  client_order_id: 'tealstreet-client-id',
  product_id: SPOT_MARKET.id,
  side: 'BUY',
  status: 'OPEN',
  order_type: 'LIMIT',
  created_time: '2026-09-18T00:00:00Z',
  filled_size: '0.25',
  average_filled_price: '50010',
  total_fees: '0',
  order_configuration: {
    limit_limit_gtc: {
      base_size: '1.25',
      limit_price: '50000',
    },
  },
  ...overrides,
});

const createExchange = (markets: Market[] = [SPOT_MARKET]) => {
  const store = new DefaultStore();
  store.update({ markets });
  const exchange = new CoinbaseExchange(
    'coinbase' as any,
    { key: 'api-key', secret: 'api-secret', tealstreetProxy: false, tealstreetTunnel: false } as ExchangeOptions,
    store,
  );
  vi.spyOn(exchange, 'getSupportedTypes').mockResolvedValue(['spot']);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};

describe('Coinbase REST open-order polling', () => {
  it('fetches paginated open spot orders and maps documented REST order fields', async () => {
    const { exchange } = createExchange();
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          orders: [makeRestOrder()],
          has_next: true,
          cursor: 'next-page',
        },
      })
      .mockResolvedValueOnce({
        data: {
          orders: [
            makeRestOrder({
              order_id: 'second-order-id',
              side: 'SELL',
              filled_size: '0',
              order_configuration: {
                stop_limit_stop_limit_gtc: {
                  base_size: '0.5',
                  limit_price: '49000',
                  stop_price: '49500',
                },
              },
            }),
            makeRestOrder({
              order_id: 'quote-sized-order-id',
              filled_size: '0',
              order_configuration: {
                limit_limit_gtc: {
                  quote_size: '100',
                  limit_price: '50000',
                },
              },
            }),
          ],
          has_next: false,
        },
      });
    exchange.xhr = { get } as unknown as Axios;

    const orders = await exchange.fetchOrders();

    expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.LIST_ORDERS, {
      params: {
        product_type: 'SPOT',
        order_status: OPEN_ORDER_STATUSES,
        limit: 100,
      },
    });
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.LIST_ORDERS, {
      params: {
        product_type: 'SPOT',
        order_status: OPEN_ORDER_STATUSES,
        limit: 100,
        cursor: 'next-page',
      },
    });
    expect(orders).toEqual([
      expect.objectContaining({
        id: 'coinbase-order-id',
        parentId: 'tealstreet-client-id',
        status: OrderStatus.Open,
        symbol: 'BTC-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 50_000,
        amount: 1.25,
        filled: 0.25,
        remaining: 1,
        extra: { remainingNative: 1 },
      }),
      expect.objectContaining({
        id: 'second-order-id',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 49_500,
        amount: 0.5,
        filled: 0,
        remaining: 0.5,
      }),
      expect.objectContaining({
        id: 'quote-sized-order-id',
        type: OrderType.Limit,
        price: 50_000,
        amount: 0.002,
        filled: 0,
        remaining: 0.002,
      }),
    ]);
  });

  it('preserves last-known-good orders when REST polling is rate limited', async () => {
    const { exchange, store } = createExchange();
    const existingOrder = {
      id: 'existing-order',
      status: OrderStatus.Open,
      symbol: 'BTC-USD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 50_000,
      amount: 1,
      filled: 0,
      remaining: 1,
      reduceOnly: false,
      info: {},
      lastUpdate: 1,
    };
    store.update({ orders: [existingOrder] });

    const rateLimitError = new Error('GET /orders/historical/batch failed with 429') as Error & {
      response?: { status: number; data?: { message?: string } };
    };
    rateLimitError.response = { status: 429, data: { message: 'rate limited' } };
    exchange.xhr = { get: vi.fn().mockRejectedValue(rateLimitError) } as unknown as Axios;

    await expect(exchange.fetchOrders()).resolves.toEqual([existingOrder]);
    expect(exchange.emitAndLogError).toHaveBeenCalledWith(rateLimitError, 'rate limited');
  });

  it('uses the inherited reconnect snapshot fetch instead of a Coinbase no-op', async () => {
    const { exchange, store } = createExchange();
    const orders = [
      {
        id: 'snapshot-order',
        status: OrderStatus.Open,
        symbol: 'BTC-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 50_000,
        amount: 1,
        filled: 0,
        remaining: 1,
        reduceOnly: false,
        info: {},
        lastUpdate: 1,
      },
    ];
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue(orders);

    await exchange.fetchAndSetPrivateSnapshot();

    expect(store.orders).toEqual(orders);
    expect(store.loaded.orders).toBe(true);
  });
});
