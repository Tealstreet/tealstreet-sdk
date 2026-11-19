import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderType } from '../../types';
import { OrderlyExchange } from '../reference/orderly/orderly.exchange';

describe('audit regression: Orderly trailing-stop placement', () => {
  it('fails closed with an emitted error instead of posting an empty positional TP/SL', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new FrameStore(),
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
