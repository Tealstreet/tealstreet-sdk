import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createAPI, createPublicAPI } from './reference/binance/binance.api';
import { ENDPOINTS } from '../exchanges/binance/binance.types';
import { createAPI as createInverseAPI, createPublicAPI as createInversePublicAPI } from './reference/binance/binanceinverse.api';
import { ENDPOINTS as INVERSE_ENDPOINTS } from '../exchanges/binance/binanceinverse.types';
import { createPAPI, createPublicAPI as createPmPublicAPI } from './reference/binance/binancepm.api';
import { ENDPOINTS as PM_ENDPOINTS } from '../exchanges/binance/binancepm.types';

/**
 * A hosted exchange-server signs through the browser, one round trip per
 * request. Binance's interceptors already leave the livenet listenKey and every
 * public endpoint unsigned, so asking the device for them is pure latency on a
 * link that may be slow. Tagged requests skip that round trip.
 */

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
      return Promise.resolve({ data: {}, status: 200, statusText: 'OK', headers: {}, config });
    },
  });
  return captured!;
};

const isTagged = (config: InternalAxiosRequestConfig) =>
  (config as unknown as Record<string, unknown>).safeCexSkipRemoteSigning === true;

const LOCAL = { key: 'k', secret: 's', testnet: false };
// The hosted exchange-server holds the key but no secret, and signs via the device.
const HOSTED = { ...LOCAL, secret: '', remoteSigner: { sign: async () => ({}), accountName: 'A' } };

describe('binance remote-signing skip tags', () => {
  it('tags the livenet listenKey and keeps the api key header', async () => {
    const config = await capture(createAPI(HOSTED as any, stubExchange('binance')), ENDPOINTS.LISTEN_KEY, 'post');
    expect(isTagged(config)).toBe(true);
    expect(config.headers['X-MBX-APIKEY']).toBe('k');
  });

  it('does not tag the testnet listenKey, which is signed', async () => {
    const config = await capture(
      createAPI({ ...LOCAL, testnet: true } as any, stubExchange('binance')),
      ENDPOINTS.LISTEN_KEY,
      'post',
    );
    expect(isTagged(config)).toBe(false);
  });

  it.each([ENDPOINTS.MARKETS, ENDPOINTS.TICKERS_24H])('tags public %s on both clients', async (url) => {
    expect(isTagged(await capture(createAPI(HOSTED as any, stubExchange('binance')), url))).toBe(true);
    expect(isTagged(await capture(createPublicAPI(HOSTED as any, stubExchange('binance')), url))).toBe(true);
  });

  it.each([ENDPOINTS.ACCOUNT, ENDPOINTS.OPEN_ORDERS])('never tags private %s in the hosted shape', async (url) => {
    expect(isTagged(await capture(createAPI(HOSTED as any, stubExchange('binance')), url))).toBe(false);
  });

  it('binancepm: tags listenKey and public, never a private endpoint', async () => {
    const papi = createPAPI(HOSTED as any, stubExchange('binancepm'));
    expect(isTagged(await capture(papi, PM_ENDPOINTS.LISTEN_KEY, 'post'))).toBe(true);
    expect(
      isTagged(await capture(createPmPublicAPI(HOSTED as any, stubExchange('binancepm')), PM_ENDPOINTS.MARKETS)),
    ).toBe(true);
    expect(isTagged(await capture(papi, PM_ENDPOINTS.ACCOUNT))).toBe(false);
  });

  it('binanceinverse: tags listenKey and public, never a private endpoint', async () => {
    const dapi = createInverseAPI(HOSTED as any, stubExchange('binanceinverse'));
    expect(isTagged(await capture(dapi, INVERSE_ENDPOINTS.LISTEN_KEY, 'post'))).toBe(true);
    expect(
      isTagged(await capture(createInversePublicAPI(HOSTED as any, stubExchange('binanceinverse')), INVERSE_ENDPOINTS.MARKETS)),
    ).toBe(true);
    expect(isTagged(await capture(dapi, INVERSE_ENDPOINTS.ACCOUNT))).toBe(false);
  });
});
