import { describe, expect, it, vi } from 'vitest';

import { createAPI } from '../reference/bitget/bitget.api';
import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';

const createExchange = () =>
  new BitgetExchange(
    'bitget',
    { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
    new DefaultStore(),
  );

const runResponseInterceptor = (exchange: BitgetExchange, response: any) => {
  const xhr = createAPI({ key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase' } as any, exchange);
  const handlers = (xhr.interceptors.response as any).handlers.filter(Boolean);
  const fulfilled = handlers[handlers.length - 1].fulfilled;
  return fulfilled(response);
};

describe('audit regression: Bitget account validation', () => {
  it('returns the upstream message when the account-type probe receives an error envelope', async () => {
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
    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_SETTINGS_V3);
  });

  it('promotes supported linear accounts to UTA when a Classic fallback returns 40085', async () => {
    const exchange = createExchange();
    const get = vi
      .fn()
      .mockRejectedValueOnce(new Error('temporary account-type probe failure'))
      .mockResolvedValueOnce({
        data: {
          code: '40085',
          msg: 'You are in Unified Account mode, and the Classic Account API is not supported at this time',
          data: null,
        },
      })
      .mockResolvedValueOnce({
        data: {
          code: '00000',
          data: {
            accountEquity: '100',
            effEquity: '95',
            unrealisedPnl: '1',
            assets: [{ coin: 'USDT', available: '95', equity: '100' }],
          },
        },
      });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: '',
      balance: expect.objectContaining({
        free: 95,
        total: 99,
        upnl: 1,
      }),
    });
    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(get).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.ACCOUNT_SETTINGS_V3);
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.BALANCE, {
      params: { productType: 'usdt-futures' },
    });
    expect(get).toHaveBeenNthCalledWith(3, ENDPOINTS.UTA_ACCOUNT_ASSETS);
  });

  it('returns the helpful environment mismatch message from the account-type probe without falling through', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: '40017',
        msg: 'exchange environment is incorrect',
        data: null,
      },
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = { get } as any;

    const helpfulMsg =
      'API key environment mismatch: Your API key appears to be for Demo/Testnet trading, but Tealstreet is configured for Live/Production. Please check your Testnet setting.';

    await expect(exchange.validateAccount()).resolves.toEqual({ error: helpfulMsg });
    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_SETTINGS_V3);
  });

  it('does not emit the raw environment mismatch message from the response interceptor', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    await runResponseInterceptor(exchange, {
      data: {
        code: '40017',
        msg: 'exchange environment is incorrect',
      },
      status: 400,
      statusText: 'Bad Request',
    });

    expect(emitAndLogError).not.toHaveBeenCalled();
  });

  it('returns validation-owned signature errors without emitting from the interceptor or validateAccount', async () => {
    const exchange = createExchange();
    (exchange as any)._accountTypeDetected = true;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const api = createAPI(
      { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
      exchange,
    );

    const adapter = vi.fn(async (config: any) => ({
      data: {
        code: '40009',
        msg: 'sign signature error',
        data: null,
      },
      status: 400,
      statusText: 'Bad Request',
      headers: {},
      config,
    }));
    api.defaults.adapter = adapter;
    exchange.xhr = api;

    await expect(exchange.validateAccount()).resolves.toEqual({ error: 'sign signature error' });
    expect(emitAndLogError).not.toHaveBeenCalled();
    // Re-signed and resent once before the validation reports it.
    expect(adapter).toHaveBeenCalledTimes(2);
  });

  it('still emits resolved Bitget API errors outside account validation', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    const api = createAPI(
      { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
      exchange,
    );
    const adapter = vi.fn(async (config: any) => ({
      data: { code: '40009', msg: 'sign signature error' },
      status: 400,
      statusText: 'Bad Request',
      headers: {},
      config,
    }));
    api.defaults.adapter = adapter;

    await api.get(ENDPOINTS.BALANCE);

    // The first rejection is re-signed and resent; only the resend's failure is emitted.
    expect(adapter).toHaveBeenCalledTimes(2);
    expect(emitAndLogError).toHaveBeenCalledTimes(1);
    expect(emitAndLogError).toHaveBeenCalledWith('sign signature error (HTTP 400)');
  });
});
