import type { ExchangeOptions } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
import { DefaultStore } from '../../store/store.base';

// KuCoin rejects orders as HTTP 200 with a non-'200000' `code`. The normal
// order path (placeOrderBatch) used to emit the rejection but resolve with an
// empty id list, so awaiting callers saw "success" with no ids while the order
// never existed on the venue. It must throw, matching placeAlgoOrders.
const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new KucoinExchange(
    'kucoin' as any,
    { key: 'k', secret: 's', passphrase: 'p' } as ExchangeOptions,
    store,
  );
  return exchange;
};

describe('KuCoin normal-order rejection surfacing', () => {
  it('placeOrderBatch throws and emits on a non-200000 rejection code', async () => {
    const exchange = makeExchange();
    const errors: unknown[] = [];
    exchange.emitter.on('error', (message: unknown) => errors.push(message));
    (exchange as any).unlimitedXHR = {
      post: vi.fn().mockResolvedValue({
        data: {
          code: '330011',
          msg: "The order's margin mode does not match the selected one. Please switch and try again.",
        },
      }),
    };

    await expect(
      exchange.placeOrderBatch([{ symbol: 'DOGEUSDTM', side: 'buy', type: 'limit', size: '1', price: '0.06' }]),
    ).rejects.toThrow(/margin mode does not match/);
    expect(errors).toHaveLength(1);
    expect(String(errors[0])).toMatch(/margin mode does not match/);
  });

  it('placeOrderBatch returns the order id on a 200000 success', async () => {
    const exchange = makeExchange();
    (exchange as any).unlimitedXHR = {
      post: vi.fn().mockResolvedValue({
        data: { code: '200000', data: { orderId: 'order-1' } },
      }),
    };

    await expect(
      exchange.placeOrderBatch([{ symbol: 'DOGEUSDTM', side: 'buy', type: 'limit', size: '1', price: '0.06' }]),
    ).resolves.toEqual(['order-1']);
  });
});
