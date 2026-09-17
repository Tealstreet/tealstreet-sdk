import { describe, expect, it, vi } from 'vitest';

import { createAPI } from '../../exchanges/bitget/bitget.api';
import { BITGET_UTA_CLASSIC_UNSUPPORTED_MESSAGE, BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
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
    expect(emitAndLogError).toHaveBeenCalledWith('Bad API key, API secret or incorrect IP whitelist.');
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_SETTINGS_V3);
  });

  it('returns an actionable UTA message when the account-type probe rejects unified accounts', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: '40085',
        msg: 'You are in Unified Account mode, and the Classic Account API is not supported at this time',
        data: null,
      },
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: BITGET_UTA_CLASSIC_UNSUPPORTED_MESSAGE,
    });
    expect(emitAndLogError).toHaveBeenCalledWith(BITGET_UTA_CLASSIC_UNSUPPORTED_MESSAGE);
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_SETTINGS_V3);
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
    expect(emitAndLogError).toHaveBeenCalledWith(helpfulMsg);
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
});
