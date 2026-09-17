import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createAPI } from '../../exchanges/coinbase/coinbase.api';
import { ENDPOINTS } from '../../exchanges/coinbase/coinbase.types';

const stubExchange = () =>
  ({
    name: 'coinbase',
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
    emitter: { emit: () => undefined },
    recordAuthSuccess: () => undefined,
    recordAuthFailure: () => undefined,
  }) as any;

const buildXhr = (options: Record<string, unknown>): AxiosInstance =>
  createAPI({ testnet: false, key: 'api-key', secret: 'api-secret', ...options } as any, stubExchange());

const capture = async (xhr: AxiosInstance, url: string) => {
  let captured: InternalAxiosRequestConfig | undefined;
  await xhr.request({
    method: 'GET',
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

const HOSTED = {
  key: 'api-key',
  secret: '',
  remoteSigner: {
    accountName: 'hosted-account',
    sign: async () => ({}),
    isAvailable: () => true,
    onAvailabilityChange: () => () => undefined,
  },
};

describe('Coinbase REST signing and public endpoint tagging', () => {
  it('tags the public INTX instruments endpoint and leaves it unsigned', async () => {
    const config = await capture(buildXhr({}), ENDPOINTS.INTX_INSTRUMENTS);

    expect(isTagged(config)).toBe(true);
    expect(config.headers.Authorization).toBeUndefined();
    expect(config.headers['Coinbase-Authorization']).toBeUndefined();
  });

  it('tags the public INTX instruments endpoint in hosted keyless mode', async () => {
    expect(isTagged(await capture(buildXhr(HOSTED), ENDPOINTS.INTX_INSTRUMENTS))).toBe(true);
  });

  it('never tags private Advanced Trade endpoints in hosted keyless mode', async () => {
    for (const endpoint of [ENDPOINTS.MARKETS, ENDPOINTS.LIST_PORTFOLIOS, ENDPOINTS.PORTFOLIO_BREAKDOWN]) {
      const config = await capture(buildXhr(HOSTED), endpoint);

      expect(isTagged(config)).toBe(false);
      expect(config.headers.Authorization).toBeUndefined();
      expect(config.headers['Coinbase-Authorization']).toBeUndefined();
    }
  });
});
