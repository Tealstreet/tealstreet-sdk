import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../../exchanges/orderly/orderly.exchange';
import { ENDPOINTS } from '../../exchanges/orderly/orderly.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType, PositionMode } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'PERP_ETH_USDC',
        symbol: 'ETHUSDC',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 } },
      } as any,
    ],
  });
  const exchange = new OrderlyExchange(
    'orderly' as any,
    { key: 'key', secret: 'ed25519:secret', applicationId: 'account-id' } as any,
    store,
  );
  vi.spyOn(exchange, 'getPositionMode').mockResolvedValue(PositionMode.OneWay);
  vi.spyOn(exchange as any, 'resolveMarginTypeForOrder').mockReturnValue(MarginType.Cross);
  return exchange;
};

describe('Orderly regular order clientOrderId', () => {
  it('sends a caller-provided client_order_id on one native regular payload', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { data: { order_id: 13 } } });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'ETHUSDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 2,
        price: 100,
        clientOrderId: 'caller-client-id',
      }),
    ).resolves.toEqual(['13']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({ client_order_id: 'caller-client-id', order_quantity: 2 }),
    );
  });

  it('rejects caller IDs when one safe-cex order would split into multiple native Orderly payloads', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'ETHUSDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 21,
        price: 100,
        clientOrderId: 'caller-client-id',
      }),
    ).rejects.toThrow('Orderly clientOrderId cannot be reused across split native order payloads');

    expect(emitAndLogError).toHaveBeenCalledWith(
      'Orderly clientOrderId cannot be reused across split native order payloads',
    );
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects an empty caller clientOrderId on regular orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'ETHUSDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 2,
        price: 100,
        clientOrderId: '',
      }),
    ).rejects.toThrow('Orderly clientOrderId must be a non-empty string');

    expect(emitAndLogError).toHaveBeenCalledWith('Orderly clientOrderId must be a non-empty string');
    expect(post).not.toHaveBeenCalled();
  });
});
