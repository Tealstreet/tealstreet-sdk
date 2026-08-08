import type { Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BingxExchange } from '../../exchanges/bingx/bingx.exchange';
import { ENDPOINTS } from '../../exchanges/bingx/bingx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, PositionMode } from '../../types';

const market: Market = {
  id: 'BTC-USDT',
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
  const exchange = new BingxExchange('bingx', { key: 'key', secret: 'secret' }, store);
  (exchange as any)._accountPositionMode = PositionMode.OneWay;
  return exchange;
};

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: market.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 0.5,
  price: 50_000,
  ...overrides,
});

describe('BingX regular order clientOrderId support', () => {
  it('uses a caller clientOrderId for single native regular placeOrder payloads', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: 0, data: { order: { orderID: 'order-1' } } } });
    exchange.unlimitedXHR = { post } as any;

    await expect(exchange.placeOrder(order({ clientOrderId: 'client01' }))).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER, expect.objectContaining({ clientOrderId: 'client01' }));
  });

  it('preserves generated lowercase IDs when no caller ID is supplied', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: 0, data: { order: { orderID: 'order-1' } } } });
    exchange.unlimitedXHR = { post } as any;

    await exchange.placeOrder(order());

    expect(post.mock.calls[0][1].clientOrderId).toMatch(/^ts[a-z0-9]+$/);
  });

  it('rejects caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.unlimitedXHR = { post } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder(order({ amount: 2.25, clientOrderId: 'client01' })),
    ).rejects.toThrow('BingX clientOrderId is supported only when one safe-cex order maps to one native order');
    expect(post).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(
      'BingX clientOrderId is supported only when one safe-cex order maps to one native order',
    );
  });

  it('rejects caller clientOrderId values that BingX would mutate', async () => {
    const exchange = createExchange();

    await expect(exchange.placeOrder(order({ clientOrderId: 'Client01' }))).rejects.toThrow(
      'BingX clientOrderId must be 1-40 lowercase characters',
    );
  });
});
