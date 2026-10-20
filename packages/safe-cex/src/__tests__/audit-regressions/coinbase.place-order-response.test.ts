import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';
import { Market, Order, OrderSide, OrderStatus, OrderType } from '../../types';

const SPOT_MARKET: Market = {
  id: 'ARB-USDC',
  symbol: 'ARB-USDC',
  base: 'ARB',
  quote: 'USDC',
  active: true,
  isSpot: true,
  isExpiring: false,
  precision: { amount: 0.01, price: 0.0001 },
  limits: {
    amount: { min: 0.01, max: 1_000_000 },
    leverage: { min: 1, max: 1 },
  },
  contractSize: 1,
  info: { product_type: 'SPOT' },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [SPOT_MARKET] });
  const exchange = new CoinbaseExchange(
    'coinbase' as any,
    { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
    store,
  );
  vi.spyOn(exchange as any, 'formatCreateOrder').mockReturnValue({ client_order_id: 'client-order-id' });
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return { exchange, store };
};

const openOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'documented-order-id',
  status: OrderStatus.Open,
  symbol: 'ARB-USDC',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 0.19695,
  amount: 100,
  filled: 0,
  remaining: 100,
  reduceOnly: false,
  lastUpdate: 1_790_013_277_868,
  ...overrides,
});

describe('Coinbase create-order response audit regression', () => {
  it('returns the documented nested success response order ID', async () => {
    const { exchange } = createExchange();
    const post = vi.fn().mockResolvedValue({
      data: { success: true, success_response: { order_id: 'documented-order-id' } },
    });
    exchange.xhr = { post } as any;
    vi.spyOn(exchange as any, 'fetchOpenOrdersSnapshot').mockResolvedValue([]);

    await expect(exchange.placeOrder({} as any)).resolves.toEqual(['documented-order-id']);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER, { client_order_id: 'client-order-id' });
    exchange.dispose();
  });

  it('surfaces the documented new-order failure reason', async () => {
    const { exchange } = createExchange();
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: {
          success: false,
          failure_reason: 'legacy reason',
          error_response: { new_order_failure_reason: 'documented reason' },
        },
      }),
    } as any;

    await expect(exchange.placeOrder({} as any)).rejects.toThrow('documented reason');
  });

  it('seeds complete spot limit orders locally and reconciles through REST after writes', async () => {
    vi.useFakeTimers();
    try {
      const { exchange, store } = createExchange();
      vi.mocked((exchange as any).formatCreateOrder).mockReturnValue({
        client_order_id: 'client-order-id',
        product_id: 'ARB-USDC',
        side: 'BUY',
        order_configuration: {
          limit_limit_gtc: {
            base_size: '100',
            limit_price: '0.19695',
            post_only: true,
          },
        },
      });
      const post = vi.fn().mockResolvedValue({
        data: {
          success: true,
          success_response: { order_id: 'documented-order-id' },
          order_configuration: {
            limit_limit_gtc: {
              base_size: '100',
              limit_price: '0.19695',
              post_only: true,
            },
          },
        },
      });
      exchange.xhr = { post } as any;
      const fetchOpenOrdersSnapshot = vi
        .spyOn(exchange as any, 'fetchOpenOrdersSnapshot')
        .mockResolvedValue([openOrder()]);

      await expect(exchange.placeOrder({} as any)).resolves.toEqual(['documented-order-id']);

      expect(store.orders).toEqual([
        expect.objectContaining({
          id: 'documented-order-id',
          parentId: 'client-order-id',
          status: OrderStatus.Open,
          symbol: 'ARB-USDC',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          price: 0.19695,
          amount: 100,
          filled: 0,
          remaining: 100,
        }),
      ]);
      expect(fetchOpenOrdersSnapshot).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(2_000);

      expect(fetchOpenOrdersSnapshot).toHaveBeenCalledOnce();
      expect(store.orders).toEqual([openOrder()]);
      exchange.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it('marks Coinbase private WS degraded when no user stream update follows a REST write', async () => {
    vi.useFakeTimers();
    try {
      const { exchange, store } = createExchange();
      const warning = vi.spyOn(exchange.emitter, 'emit');
      exchange.xhr = {
        post: vi.fn().mockResolvedValue({
          data: { success: true, success_response: { order_id: 'documented-order-id' } },
        }),
      } as any;
      vi.spyOn(exchange as any, 'fetchOpenOrdersSnapshot').mockResolvedValue([openOrder()]);

      await expect(exchange.placeOrder({} as any)).resolves.toEqual(['documented-order-id']);
      await vi.advanceTimersByTimeAsync(8_000);

      expect(store.socketHealth?.privateDegradedReason).toContain('Coinbase private WS is not receiving live order updates');
      expect(warning).toHaveBeenCalledWith(
        'warning',
        expect.stringContaining('Coinbase private WS is not receiving live order updates'),
        { forceToast: true },
      );
      exchange.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps Coinbase private WS healthy when a user stream update follows a REST write', async () => {
    vi.useFakeTimers();
    try {
      const { exchange, store } = createExchange();
      const warning = vi.spyOn(exchange.emitter, 'emit');
      exchange.xhr = {
        post: vi.fn().mockResolvedValue({
          data: { success: true, success_response: { order_id: 'documented-order-id' } },
        }),
      } as any;
      vi.spyOn(exchange as any, 'fetchOpenOrdersSnapshot').mockResolvedValue([openOrder()]);

      await expect(exchange.placeOrder({} as any)).resolves.toEqual(['documented-order-id']);
      await vi.advanceTimersByTimeAsync(1_000);
      exchange.markPrivateUserStreamMessageReceived();
      await vi.advanceTimersByTimeAsync(7_000);

      expect(store.socketHealth?.privateDegradedReason).toBeUndefined();
      expect(warning).not.toHaveBeenCalledWith(
        'warning',
        expect.stringContaining('Coinbase private WS is not receiving live order updates'),
        { forceToast: true },
      );
      exchange.dispose();
    } finally {
      vi.useRealTimers();
    }
  });
});
