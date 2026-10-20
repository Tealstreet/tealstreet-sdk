import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, OrderSide, OrderTimeInForce, OrderType, PositionMode } from '../../types';

const MARKET: Market = {
  id: 'BTC/USDC:USDC',
  symbol: 'BTCUSDC',
  base: 'BTC',
  quote: 'USDC',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });

  const exchange = new OrderlyExchange(
    'kodiakfi',
    { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
    store,
  );
  (exchange as any)._accountPositionMode = PositionMode.OneWay;

  return exchange;
};

describe('audit regression: Orderly expected order rejection logging', () => {
  it('suppresses noisy console traces for expected /v1/order validation rejections', async () => {
    const exchange = createExchange();
    const message = 'The order value should be greater or equal to 10.00000000.';
    const error = { response: { data: { message } } };
    exchange.xhr = {
      post: vi.fn().mockRejectedValue(error),
    } as unknown as Axios;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDC',
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 0.001,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).rejects.toBe(error);

    expect(emitAndLogError).toHaveBeenCalledWith(error, message, { suppressConsoleLog: true });
  });

  it('suppresses noisy console traces for expected positional TP/SL validation rejections', async () => {
    const exchange = createExchange();
    const message = 'The TP_SL side should be opposite to position side.';
    exchange.xhr = {
      post: vi.fn().mockRejectedValue({ response: { data: { message } } }),
    } as unknown as Axios;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    await expect(
      exchange.placePositionalAlgoOrder({
        symbol: 'BTCUSDC',
        side: OrderSide.Sell,
        stopLoss: 95,
      }),
    ).resolves.toEqual([]);

    expect(emitAndLogError).toHaveBeenCalledWith(message, undefined, { suppressConsoleLog: true });
  });
});
