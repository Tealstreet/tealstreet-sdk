import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../../exchanges/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../../exchanges/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';

const createExchange = () => {
  const exchange = new CoinbaseExchange(
    'coinbase' as any,
    { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
    new DefaultStore(),
  );
  vi.spyOn(exchange as any, 'formatCreateOrder').mockReturnValue({ client_order_id: 'client-order-id' });
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
  return exchange;
};

describe('Coinbase create-order response audit regression', () => {
  it('returns the documented nested success response order ID', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({
      data: { success: true, success_response: { order_id: 'documented-order-id' } },
    });
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrder({} as any)).resolves.toEqual(['documented-order-id']);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER, { client_order_id: 'client-order-id' });
  });

  it('surfaces the documented new-order failure reason', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: {
          success: false,
          failure_reason: 'legacy reason',
          error_response: { new_order_failure_reason: 'documented reason' },
        },
      }),
    } as any;

    await expect(exchange.placeOrder({} as any)).rejects.toThrow('documented reason');
  });
});
