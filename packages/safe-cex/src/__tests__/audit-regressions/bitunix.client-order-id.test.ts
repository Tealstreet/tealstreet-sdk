import type { Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BitunixExchange } from '../../exchanges/bitunix/bitunix.exchange';
import { ENDPOINTS } from '../../exchanges/bitunix/bitunix.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';

const market: Market = {
  id: 'BTC/USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1 }, leverage: { min: 1, max: 100 } },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [market] });
  return new BitunixExchange('bitunix', { key: 'key', secret: 'secret' }, store);
};

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: market.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 0.5,
  price: 50_000,
  timeInForce: OrderTimeInForce.PostOnly,
  ...overrides,
});

describe('Bitunix regular order clientOrderId support', () => {
  it('uses caller clientOrderId as clientId and moves local caches to that ID', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: 0, data: { orderId: 'order-1' } } });
    exchange.unlimitedXHR = { post } as any;

    await expect(exchange.placeOrder(order({ clientOrderId: 'custom-id-1' }))).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER, expect.objectContaining({ clientId: 'custom-id-1' }));
    expect(exchange.orderTifCache.get('custom-id-1')).toBe(OrderTimeInForce.PostOnly);
    expect(exchange.orderReduceOnlyCache.get('custom-id-1')).toBe(false);
  });

  it('preserves generated clientId when no caller ID is supplied', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: 0, data: { orderId: 'order-1' } } });
    exchange.unlimitedXHR = { post } as any;

    await exchange.placeOrder(order());

    expect(post.mock.calls[0][1].clientId).toMatch(/^ts_/);
  });

  it('rejects caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.unlimitedXHR = { post } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder(order({ amount: 2.25, clientOrderId: 'custom-id-1' })),
    ).rejects.toThrow('Bitunix clientId is supported only when one safe-cex order maps to one native order');
    expect(post).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(
      'Bitunix clientId is supported only when one safe-cex order maps to one native order',
    );
  });

  it('rejects an empty caller clientOrderId on regular orders', async () => {
    const exchange = createExchange();

    await expect(exchange.placeOrder(order({ clientOrderId: '' }))).rejects.toThrow(
      'Bitunix clientId must be a non-empty string',
    );
  });
});
