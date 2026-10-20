import type { Axios } from 'axios';
import type { LIST_ORDERS_RESPONSE } from '../reference/coinbase/coinbase.types';

import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, LogSeverity, Market, OrderSide, OrderStatus, OrderType } from '../../types';

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

const SPOT_USDC_MARKET: Market = {
  ...SPOT_MARKET,
  id: 'BTC-USDC',
  symbol: 'BTC-USDC',
  quote: 'USDC',
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

type ListOrdersPage = {
  orders: LIST_ORDERS_RESPONSE['orders'];
  has_next: boolean;
  cursor?: string;
};

const emptyListOrdersPage = (): ListOrdersPage => ({
  orders: [],
  has_next: false,
});

const mockListOrdersByStatus = (pagesByStatus: Partial<Record<string, ListOrdersPage[]>>) => {
  const nextPageByStatus = new Map<string, number>();

  return vi.fn((_endpoint: string, config: { params: { order_status: string } }) => {
    const status = config.params.order_status;
    const pages = pagesByStatus[status] ?? [emptyListOrdersPage()];
    const index = nextPageByStatus.get(status) ?? 0;
    nextPageByStatus.set(status, index + 1);
    return Promise.resolve({ data: pages[Math.min(index, pages.length - 1)] });
  });
};

describe('Coinbase REST open-order polling', () => {
  it('fetches paginated open spot orders and maps documented REST order fields', async () => {
    const { exchange } = createExchange();
    const get = mockListOrdersByStatus({
      OPEN: [
        {
          orders: [makeRestOrder()],
          has_next: true,
          cursor: 'next-page',
        },
        {
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
      ],
    });
    exchange.xhr = { get } as unknown as Axios;

    const orders = await exchange.fetchOrders();

    expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.LIST_ORDERS, {
      params: {
        product_type: 'SPOT',
        order_status: 'PENDING',
        limit: 100,
      },
    });
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.LIST_ORDERS, {
      params: {
        product_type: 'SPOT',
        order_status: 'OPEN',
        limit: 100,
      },
    });
    expect(get).toHaveBeenNthCalledWith(3, ENDPOINTS.LIST_ORDERS, {
      params: {
        product_type: 'SPOT',
        order_status: 'OPEN',
        limit: 100,
        cursor: 'next-page',
      },
    });
    expect(get.mock.calls.map(([, config]) => config.params.order_status)).toEqual([
      'PENDING',
      'OPEN',
      'OPEN',
      'QUEUED',
      'CANCEL_QUEUED',
      'EDIT_QUEUED',
    ]);
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

  it('rejects rate-limited REST polling without mutating state or surfacing auth-looking errors', async () => {
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
    rateLimitError.response = { status: 429, data: { message: 'authentication failure' } };
    exchange.xhr = { get: vi.fn().mockRejectedValue(rateLimitError) } as unknown as Axios;
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await expect(exchange.fetchOrders()).rejects.toBe(rateLimitError);
    expect(store.orders).toEqual([existingOrder]);
    expect(store.loaded.orders).toBe(false);
    expect(exchange.emitAndLogError).not.toHaveBeenCalledWith(rateLimitError, 'authentication failure');
    expect(log).toHaveBeenCalledWith(expect.stringContaining('rate limited (429)'), LogSeverity.Warning);
    expect((exchange as any).nextOrdersPollAt).toBeGreaterThan(Date.now() + 50_000);
  });

  it('deduplicates repeated skip reasons into one summary error per poll', async () => {
    const { exchange } = createExchange();
    const get = mockListOrdersByStatus({
      OPEN: [
        {
          orders: [
            makeRestOrder({ order_id: 'unknown-1', product_id: 'UNKNOWN-PROD' }),
            makeRestOrder({ order_id: 'unknown-2', product_id: 'UNKNOWN-PROD' }),
            makeRestOrder({ order_id: 'unknown-3', product_id: 'OTHER-PROD' }),
            makeRestOrder({
              order_id: 'bad-shape-1',
              order_configuration: undefined,
            }),
            makeRestOrder({
              order_id: 'bad-shape-2',
              order_configuration: undefined,
            }),
          ],
          has_next: false,
        },
      ],
    });
    exchange.xhr = { get } as unknown as Axios;

    const orders = await exchange.fetchOrders();

    expect(orders).toEqual([]);
    // One line per reason for the whole poll — not one line per skipped order.
    const calls = (exchange.emitAndLogError as any).mock.calls;
    const messages = calls.map((call: unknown[]) => call[0]);
    const missingMarketCalls = messages.filter((m: string) => m?.startsWith?.('Coinbase orders skipped, missing market'));
    const unsupportedShapeCalls = messages.filter((m: string) =>
      m?.startsWith?.('Coinbase orders skipped, unsupported REST shape'),
    );
    expect(missingMarketCalls).toHaveLength(1);
    expect(unsupportedShapeCalls).toHaveLength(1);
    expect(missingMarketCalls[0]).toContain('UNKNOWN-PROD (2)');
    expect(missingMarketCalls[0]).toContain('OTHER-PROD (1)');
    expect(unsupportedShapeCalls[0]).toContain('BTC-USD (2)');
  });

  it('maps REST open-order USDC aliases to loaded USD markets', async () => {
    const { exchange } = createExchange();
    const get = mockListOrdersByStatus({
      OPEN: [
        {
          orders: [makeRestOrder({ order_id: 'usdc-alias-rest-order', product_id: 'BTC-USDC' })],
          has_next: false,
        },
      ],
    });
    exchange.xhr = { get } as unknown as Axios;

    const orders = await exchange.fetchOrders();

    expect(orders).toEqual([
      expect.objectContaining({
        id: 'usdc-alias-rest-order',
        symbol: 'BTC-USD',
      }),
    ]);
    expect(exchange.emitAndLogError).not.toHaveBeenCalledWith(
      expect.stringContaining('Coinbase orders skipped, missing market'),
    );
  });

  it('prefers loaded USDC markets when REST reports equivalent USD open orders', async () => {
    const { exchange } = createExchange([SPOT_MARKET, SPOT_USDC_MARKET]);
    const get = mockListOrdersByStatus({
      OPEN: [
        {
          orders: [makeRestOrder({ order_id: 'usd-rest-order', product_id: 'BTC-USD' })],
          has_next: false,
        },
      ],
    });
    exchange.xhr = { get } as unknown as Axios;

    const orders = await exchange.fetchOrders();

    expect(orders).toEqual([
      expect.objectContaining({
        id: 'usd-rest-order',
        symbol: 'BTC-USDC',
      }),
    ]);
  });

  it('preserves fresh WS-only orders when the REST snapshot has not caught up', async () => {
    const { exchange, store } = createExchange();
    const now = Date.now();
    const freshWsOrder = {
      id: 'fresh-ws-order',
      status: OrderStatus.Open,
      symbol: 'BTC-USD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 51_000,
      amount: 1,
      filled: 0,
      remaining: 1,
      reduceOnly: false,
      info: {},
      // WS event received ~500 ms ago; REST is still behind.
      lastUpdate: now - 500,
    };
    const staleWsOrder = {
      ...freshWsOrder,
      id: 'stale-ws-order',
      // Older than the 5s REST-lag tolerance; treat REST as authoritative.
      lastUpdate: now - 10_000,
    };
    store.update({ orders: [freshWsOrder, staleWsOrder] });

    const restOrder = makeRestOrder({ order_id: 'rest-only-order', filled_size: '0' });
    const merged = (exchange as any).mergeRestOrdersWithFreshWsState([
      {
        id: restOrder.order_id,
        status: OrderStatus.Open,
        symbol: SPOT_MARKET.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 50_000,
        amount: 1.25,
        filled: 0,
        remaining: 1.25,
        reduceOnly: false,
        info: {},
        lastUpdate: now,
      },
    ]) as Array<{ id: string }>;

    const ids = merged.map((order) => order.id).sort();
    expect(ids).toEqual(['fresh-ws-order', 'rest-only-order']);
    expect(ids).not.toContain('stale-ws-order');
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

  it('does not mark orders loaded when a reconnect snapshot fetch fails', async () => {
    const { exchange, store } = createExchange();
    const rateLimitError = new Error('GET /orders/historical/batch failed with 429') as Error & {
      response?: { status: number; data?: { message?: string } };
    };
    rateLimitError.response = { status: 429, data: { message: 'rate limited' } };
    vi.spyOn(exchange, 'fetchOrders').mockRejectedValue(rateLimitError);

    await exchange.fetchAndSetPrivateSnapshot();

    expect(store.orders).toEqual([]);
    expect(store.loaded.orders).toBe(false);
  });
});
