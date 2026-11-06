import type { ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { OKXEUExchange } from './reference/okx/okxeu.exchange';
import { OKXEUDemoExchange } from './reference/okx/okxeudemo.exchange';
import { ENDPOINTS } from '../exchanges/okx/okx.types';
import { DefaultStore } from '../store/store.base';

const createExchange = (): OKXEUExchange => {
  const options: ExchangeOptions = {
    key: 'okx-key',
    secret: 'okx-secret',
    passphrase: 'okx-passphrase',
    testnet: false,
    tuning: { consoleLogging: false },
  };

  return new OKXEUExchange('okxeu', options, new DefaultStore());
};

const createDemoExchange = (): OKXEUDemoExchange => {
  const options: ExchangeOptions = {
    key: 'okx-key',
    secret: 'okx-secret',
    passphrase: 'okx-passphrase',
    testnet: false,
    tuning: { consoleLogging: false },
  };

  return new OKXEUDemoExchange('okxeudemo', options, new DefaultStore());
};

describe('OKXEUExchange validateAccount', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('validates credentials with account config instead of account position risk', async () => {
    const exchange = createExchange();
    const get = vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: { data: [{ posMode: 'net_mode', acctLv: '2' }] },
    });

    const result = await exchange.validateAccount();

    expect(result).toEqual({ error: '' });
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_CONFIG);
    expect(get).not.toHaveBeenCalledWith(ENDPOINTS.BALANCE);
  });

  it('returns the OKX error message when account config validation fails', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    vi.spyOn(exchange.xhr, 'get').mockRejectedValue({
      response: { data: { msg: 'Invalid API key' } },
    });

    const result = await exchange.validateAccount();

    expect(result).toEqual({ error: 'Invalid API key' });
    expect(exchange.emitAndLogError).toHaveBeenCalledWith('Invalid API key');
  });

  it('keeps account config validation for OKX EU Demo', async () => {
    const exchange = createDemoExchange();
    const get = vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: { data: [{ posMode: 'net_mode', acctLv: '2' }] },
    });

    const result = await exchange.validateAccount();

    expect(exchange.isDemo).toBe(true);
    expect(exchange.options.testnet).toBe(true);
    expect(exchange.region).toBe('okxeu');
    expect(result).toEqual({ error: '' });
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_CONFIG);
    expect(get).not.toHaveBeenCalledWith(ENDPOINTS.BALANCE);
  });
});
