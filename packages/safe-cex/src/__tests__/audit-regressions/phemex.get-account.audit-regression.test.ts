import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../reference/phemex/phemex.exchange';
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

  it('fails clearly when the account-position response has no user identity', async () => {
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, new DefaultStore());
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { data: { account: {} } } }) } as any;

    await expect(exchange.getAccount()).rejects.toThrow(
      'Phemex account-position response did not include a user ID',
    );
    expect(emitAndLogError).toHaveBeenCalledWith('Phemex account-position response did not include a user ID');
  });
});
