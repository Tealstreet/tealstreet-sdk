import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

describe('audit regression: Orderly trailing-stop placement', () => {
  it('fails closed with an emitted error instead of posting an empty positional TP/SL', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new DefaultStore(),
    );
    const post = vi.fn();
    exchange.xhr = { post } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    await expect(
      exchange.placeAlgoOrder({
        symbol: 'BTCUSDC',
        type: OrderType.TrailingStopLoss,
        side: OrderSide.Sell,
        amount: 1,
        price: 100,
      }),
    ).rejects.toThrow('Orderly trailing-stop placement is not supported');

    expect(emitAndLogError).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'Orderly trailing-stop placement is not supported' }),
    );
    expect(post).not.toHaveBeenCalled();
  });
});
