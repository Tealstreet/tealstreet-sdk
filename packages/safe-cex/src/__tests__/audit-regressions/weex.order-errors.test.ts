import { describe, expect, it, vi } from 'vitest';

import { WeexExchange } from '../reference/weex/weex.exchange';
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
  const exchange = new WeexExchange(
    'weex' as any,
    { key: 'key', secret: 'secret', passphrase: 'passphrase' } as any,
    store,
  );
  return exchange;
};

describe('audit regression: WEEX order error surfacing', () => {
  it.each([
    [
      { symbol: 'MISSING', type: OrderType.Market, side: OrderSide.Buy, amount: 1 },
      'Market MISSING not found',
    ],
    [
      { symbol: 'BTCUSDT', type: OrderType.StopLoss, side: OrderSide.Sell, amount: 1 },
      'Trigger price is required for WEEX TP/SL orders',
    ],
    [
      { symbol: 'BTCUSDT', type: OrderType.TrailingStopLoss, side: OrderSide.Sell, amount: 1, price: 50_000 },
      'WEEX trailing stop loss is not supported',
    ],
  ])('emits preflight failure %s before rejecting', async (order, message) => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(exchange.placeOrder(order as any)).rejects.toThrow(message);
    expect(emitAndLogError).toHaveBeenCalledOnce();
    expect(emitAndLogError).toHaveBeenCalledWith(message);
  });

  it('emits a transport failure once at the POST call site', async () => {
    const exchange = createExchange();
    const error = { response: { data: { msg: 'order rate limit reached' } } };
    exchange.xhr = { post: vi.fn().mockRejectedValue(error) } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 1 }),
    ).rejects.toBe(error);
    expect(emitAndLogError).toHaveBeenCalledOnce();
    expect(emitAndLogError).toHaveBeenCalledWith('order rate limit reached');
  });

  it('emits a response-level rejection once in the result parser', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({ data: { success: false, errorCode: '40017', errorMessage: 'bad size' } }),
    } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 1 }),
    ).rejects.toThrow('bad size');
    expect(emitAndLogError).toHaveBeenCalledOnce();
    expect(emitAndLogError).toHaveBeenCalledWith('bad size');
  });
});
