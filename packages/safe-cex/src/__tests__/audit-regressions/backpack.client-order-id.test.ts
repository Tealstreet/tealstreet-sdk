import type { ExchangeOptions, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BackpackExchange } from '../../exchanges/backpack/backpack.exchange';
import { ENDPOINTS } from '../../exchanges/backpack/backpack.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC_USDC_PERP',
        symbol: 'BTC_USDC_PERP',
        base: 'BTC',
        quote: 'USDC',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      },
    ] as any,
  });
  return new BackpackExchange(
    'backpack',
    { key: 'public-key', secret: 'secret', testnet: false } as ExchangeOptions,
    store,
  );
};

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTC_USDC_PERP',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 0.01,
  price: 50_000,
  ...overrides,
});

describe('Backpack regular order clientOrderId support', () => {
  it('maps numeric caller clientOrderId strings to Backpack clientId', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { id: 'order-1' } });
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrder(order({ clientOrderId: '4294967295' }))).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER, expect.objectContaining({ clientId: 4_294_967_295 }));
  });

  it('rejects non-uint32 caller clientOrderId strings before sending', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder(order({ clientOrderId: 'client-string' })),
    ).rejects.toThrow('Backpack clientOrderId must be a uint32 decimal string');
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects numeric caller clientOrderId values outside uint32 range', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder(order({ clientOrderId: '4294967296' })),
    ).rejects.toThrow('Backpack clientOrderId must be a uint32 decimal string');
    expect(post).not.toHaveBeenCalled();
  });
});
