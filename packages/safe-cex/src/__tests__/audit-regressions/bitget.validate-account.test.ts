import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';

const createExchange = () =>
  new BitgetExchange(
    'bitget',
    { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
    new DefaultStore(),
  );

describe('audit regression: Bitget account validation', () => {
  it('returns the upstream message when validation receives an HTTP-200 error envelope', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: '40037',
        msg: 'Bad API key, API secret or incorrect IP whitelist.',
        data: null,
      },
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: 'Bad API key, API secret or incorrect IP whitelist.',
    });
    expect(emitAndLogError).toHaveBeenCalledWith('Bad API key, API secret or incorrect IP whitelist.');
    expect(get).toHaveBeenCalledWith(ENDPOINTS.BALANCE, { params: { productType: 'usdt-futures' } });
  });
});
