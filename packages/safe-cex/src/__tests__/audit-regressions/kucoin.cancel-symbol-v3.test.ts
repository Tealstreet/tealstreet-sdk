import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
import { ENDPOINTS } from '../reference/kucoin/kucoin.types';

describe('KuCoin v3 symbol cancellation', () => {
  it('cancels symbol orders through the current v3 endpoint', async () => {
    const exchange = new KucoinExchange(
      'kucoin',
      { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false },
      new FrameStore(),
    );
    const deleteRequest = vi.fn().mockResolvedValue({ data: { code: '200000' } });
    exchange.unlimitedXHR = { delete: deleteRequest } as unknown as Axios;

    await exchange.cancelSymbolOrders('BTCUSDT');

    expect(ENDPOINTS.CANCEL_SYMBOL_ORDERS).toBe('/api/v3/orders');
    expect(deleteRequest).toHaveBeenCalledWith('/api/v3/orders', { params: { symbol: 'BTCUSDT' } });
  });
});
