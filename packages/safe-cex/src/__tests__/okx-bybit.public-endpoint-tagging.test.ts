import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createAPI as createBybitAPI, shouldRetryBybitRequest } from './reference/bybit/bybit.api';
import { createAPI as createOkxAPI } from './reference/okx/okx.api';

const stubExchange = (name: string) =>
  ({
    name,
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
    emitter: { emit: () => undefined },
    recordAuthSuccess: () => undefined,
    recordAuthFailure: () => undefined,
  }) as any;

const capture = async (xhr: AxiosInstance, url: string, method: 'get' | 'post' = 'get') => {
  let captured: InternalAxiosRequestConfig | undefined;
  await xhr.request({
    method,
    url,
    adapter: (config: any) => {
      captured = config;
      return Promise.resolve({ data: { code: '0', retCode: 0 }, status: 200, statusText: 'OK', headers: {}, config });
    },
  });
  return captured!;
};

const isTagged = (config: InternalAxiosRequestConfig) =>
  (config as unknown as Record<string, unknown>).safeCexSkipRemoteSigning === true;

const LOCAL_KEYS = { key: 'k', secret: 's', passphrase: 'p' };
// The hosted exchange-server holds the key but no secret, and signs via the device.
const HOSTED = { ...LOCAL_KEYS, secret: '', remoteSigner: { sign: async () => ({}), accountName: 'A' } };

const okx = (options: Record<string, unknown>) =>
  createOkxAPI({ testnet: false, ...options } as any, stubExchange('okx'));
const bybit = (options: Record<string, unknown>) =>
  createBybitAPI({ testnet: false, ...options } as any, stubExchange('bybit'));

describe('okx public endpoint tagging', () => {
  it.each(['/api/v5/public/instruments', '/api/v5/market/tickers', '/api/v5/market/candles'])(
    'tags %s',
    async (url) => {
      expect(isTagged(await capture(okx(LOCAL_KEYS), url))).toBe(true);
    },
  );

  // The whole point of tagging by fall-through. `/market/*` does not match OKX's
  // own public check, so a key-holding client signs it — and must keep doing so.
  it('still signs /api/v5/market/tickers locally while tagging it', async () => {
    const config = await capture(okx(LOCAL_KEYS), '/api/v5/market/tickers');

    expect(config.headers['OK-ACCESS-SIGN']).toBeDefined();
    expect(config.headers['OK-ACCESS-KEY']).toBe('k');
    expect(isTagged(config)).toBe(true);
  });

  it.each(['/api/v5/account/config', '/api/v5/trade/orders-pending', '/api/v5/account/positions'])(
    'never tags %s in the hosted keyless shape',
    async (url) => {
      expect(isTagged(await capture(okx(HOSTED), url))).toBe(false);
    },
  );

  it('never tags a private endpoint for a key-holding client either', async () => {
    expect(isTagged(await capture(okx(LOCAL_KEYS), '/api/v5/account/positions'))).toBe(false);
  });

  // The production path: hosted is the only shape where the tag does anything.
  it('tags /api/v5/market/tickers in the hosted keyless shape', async () => {
    expect(isTagged(await capture(okx(HOSTED), '/api/v5/market/tickers'))).toBe(true);
  });

  it.each(['/api/v5/public/instruments', '/api/v5/market/tickers', '/api/v5/account/config'])(
    'adds the simulated trading header for demo %s',
    async (url) => {
      const config = await capture(okx({ ...LOCAL_KEYS, testnet: true }), url);

      expect(config.baseURL).toBe('https://openapi.okx.com');
      expect(config.headers['x-simulated-trading']).toBe('1');
    },
  );
});

describe('bybit public endpoint tagging', () => {
  it.each(['/v5/market/tickers', '/v5/market/instruments-info', '/v5/market/kline'])('tags %s', async (url) => {
    expect(isTagged(await capture(bybit(LOCAL_KEYS), url))).toBe(true);
  });

  it('still signs /v5/market/tickers locally while tagging it', async () => {
    const config = await capture(bybit(LOCAL_KEYS), '/v5/market/tickers');

    expect(config.headers['X-BAPI-SIGN']).toBeDefined();
    expect(isTagged(config)).toBe(true);
  });

  // Kline keeps its header-stripping early return, which rebuilds the config —
  // the tag has to survive that spread or the round trip silently comes back.
  it('keeps kline unsigned and still tagged', async () => {
    const config = await capture(bybit(LOCAL_KEYS), '/v5/market/kline');

    expect(config.headers['X-BAPI-SIGN']).toBeUndefined();
    expect(isTagged(config)).toBe(true);
  });

  it('sends the Bybit Brazil site header on signed REST requests', async () => {
    const config = await capture(bybit({ ...LOCAL_KEYS, bybitSiteId: 'BRA_BTL' }), '/v5/position/list');

    expect(config.headers['x-site-id']).toBe('BRA_BTL');
    expect(config.headers['X-BAPI-SIGN']).toBeDefined();
  });

  it('keeps the Bybit Brazil site header when kline strips auth headers', async () => {
    const config = await capture(bybit({ ...LOCAL_KEYS, bybitSiteId: 'BRA_BTL' }), '/v5/market/kline');

    expect(config.headers['x-site-id']).toBe('BRA_BTL');
    expect(config.headers['X-BAPI-SIGN']).toBeUndefined();
    expect(isTagged(config)).toBe(true);
  });

  it.each(['/v5/order/realtime', '/v5/position/list', '/v5/account/wallet-balance'])(
    'never tags %s in the hosted keyless shape',
    async (url) => {
      expect(isTagged(await capture(bybit(HOSTED), url))).toBe(false);
    },
  );

  it('never tags a private endpoint for a key-holding client either', async () => {
    expect(isTagged(await capture(bybit(LOCAL_KEYS), '/v5/position/list'))).toBe(false);
  });

  it('tags /v5/market/tickers in the hosted keyless shape', async () => {
    expect(isTagged(await capture(bybit(HOSTED), '/v5/market/tickers'))).toBe(true);
  });

  it('does not retry instruments-info network errors inside the worker startup budget', () => {
    expect(
      shouldRetryBybitRequest({
        code: 'ERR_NETWORK',
        config: { url: '/v5/market/instruments-info' },
        request: {},
      }),
    ).toBe(false);
  });

  it('still retries signature drift responses', () => {
    expect(
      shouldRetryBybitRequest({
        config: { url: '/v5/order/realtime' },
        response: { data: { retMsg: 'error sign!' } },
      }),
    ).toBe(true);
  });
});
