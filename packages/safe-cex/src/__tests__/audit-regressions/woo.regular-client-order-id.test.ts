import { describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../../exchanges/woo/woo.exchange';
import { ENDPOINTS } from '../../exchanges/woo/woo.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, PositionMode } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 } },
      } as any,
    ],
    tickers: [{ id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', last: 100 } as any],
  });
  const exchange = new WOOXExchange('woo' as any, { key: 'key', secret: 'secret' } as any, store);
  vi.spyOn(exchange, 'getPositionMode').mockResolvedValue(PositionMode.OneWay);
  return exchange;
};

describe('WOO regular order clientOrderId', () => {
  it('sends a caller-provided numeric client_order_id on one native regular payload', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, order_id: 301 } });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 2,
        price: 100,
        clientOrderId: '123456',
      }),
    ).resolves.toEqual(['301']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({ client_order_id: 123456, order_quantity: 2 }),
    );
  });

  it('rejects caller IDs that cannot be represented as WOO safe integer payloads', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 2,
        price: 100,
        clientOrderId: `${Number.MAX_SAFE_INTEGER + 1}`,
      }),
    ).rejects.toThrow('WOO clientOrderId must be a JavaScript-safe integer');

    expect(emitAndLogError).toHaveBeenCalledWith('WOO clientOrderId must be a JavaScript-safe integer');
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects caller IDs when one safe-cex order would split into multiple native WOO payloads', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 21,
        price: 100,
        clientOrderId: '123456',
      }),
    ).rejects.toThrow('WOO clientOrderId cannot be reused across split native order payloads');

    expect(emitAndLogError).toHaveBeenCalledWith(
      'WOO clientOrderId cannot be reused across split native order payloads',
    );
    expect(post).not.toHaveBeenCalled();
  });
});
