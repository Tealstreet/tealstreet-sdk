import type { PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { WeexExchange } from '../../exchanges/weex/weex.exchange';
import { ENDPOINTS } from '../../exchanges/weex/weex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      },
    ] as any,
  });
  return new WeexExchange('weex' as any, { key: 'key', secret: 'secret', passphrase: 'pass' } as any, store);
};

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 0.01,
  price: 50_000,
  ...overrides,
});

describe('WEEX regular order clientOrderId support', () => {
  it('uses caller clientOrderId as newClientOrderId for regular placeOrder', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockImplementation((_endpoint: string, payload: any) =>
      Promise.resolve({
        data: { orderId: 'order-1', clientOrderId: payload.newClientOrderId, success: true },
      }),
    );
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrder(order({ clientOrderId: 'Client_01' }))).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER, expect.objectContaining({ newClientOrderId: 'Client_01' }));
  });

  it('preserves generated newClientOrderId when no caller ID is supplied', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockImplementation((_endpoint: string, payload: any) =>
      Promise.resolve({
        data: { orderId: 'order-1', clientOrderId: payload.newClientOrderId, success: true },
      }),
    );
    exchange.xhr = { post } as any;

    await exchange.placeOrder(order());

    expect(post.mock.calls[0][1].newClientOrderId).toMatch(/^ts.+/);
    expect(post.mock.calls[0][1].newClientOrderId).not.toBe('');
  });

  it('rejects caller clientOrderId values outside WEEX constraints', async () => {
    const exchange = createExchange();

    await expect(exchange.placeOrder(order({ clientOrderId: 'bad id' }))).rejects.toThrow(
      'WEEX newClientOrderId must be 1-36 characters',
    );
  });
});
