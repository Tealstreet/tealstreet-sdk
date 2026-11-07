import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

describe('audit regression: AsterDEX trailing-stop capability', () => {
  it('does not advertise trailing stops without a callback-rate input', () => {
    expect(exchangeMetadata.asterdex.supportsTrailingStopLoss).toBe(false);
  });

  it('rejects direct trailing-stop placement before posting an invalid order', async () => {
    const exchange = new AsterDEXExchange(
      'asterdex',
      { key: 'test-key', secret: 'test-secret' },
      new DefaultStore(),
    );
    const post = vi.fn();
    (exchange as any).secondaryAPI.post = post;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.TrailingStopLoss,
        amount: 1,
      }),
    ).rejects.toThrow('callbackRate is unavailable');

    expect(post).not.toHaveBeenCalled();
  });
});
