import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../../exchanges/phemex/phemex.exchange';
import { ENDPOINTS } from '../../exchanges/phemex/phemex.types';
import { DefaultStore } from '../../store/store.base';

describe('Phemex account identity audit regression', () => {
  it('reads the user ID from the documented account-position response object', async () => {
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, new DefaultStore());
    const get = vi.fn().mockResolvedValue({ data: { data: { account: { userID: 12345 } } } });
    exchange.xhr = { get } as any;

    await expect(exchange.getAccount()).resolves.toEqual({ userId: '12345' });
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { currency: 'USDT' } });
  });
});
