import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createAPI, resolveBackpackAuthWindow } from '../reference/backpack/backpack.api';
import { ENDPOINTS } from '../reference/backpack/backpack.types';

const privateKey = Buffer.alloc(32, 7).toString('base64');

const stubExchange = () =>
  ({
    name: 'backpack',
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
    emitter: { emit: () => undefined },
    emitAndLogError: () => undefined,
    recordAuthSuccess: () => undefined,
    recordAuthFailure: () => undefined,
  }) as any;

const buildXhr = (options: Record<string, unknown>): AxiosInstance =>
  createAPI({ testnet: false, ...options } as any, stubExchange());

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

const LOCAL_KEYS = { key: 'public-key', secret: privateKey };
const HOSTED = { key: 'public-key', secret: '', remoteSigner: { sign: async () => ({}), accountName: 'B' } };

describe('Backpack REST authentication window', () => {
  it('uses the documented maximum request window by default', async () => {
    const config = await capture(buildXhr(LOCAL_KEYS), ENDPOINTS.POSITIONS);

    expect(config.headers['X-Window']).toBe('60000');
    expect(config.headers['X-Signature']).toBeDefined();
    expect(isTagged(config)).toBe(false);
  });

  it('respects explicit smaller windows and clamps oversized windows', () => {
    expect(resolveBackpackAuthWindow(5000)).toBe(5000);
    expect(resolveBackpackAuthWindow(120_000)).toBe(60_000);
    expect(resolveBackpackAuthWindow(0)).toBe(60_000);
  });
});

describe('Backpack public endpoint remote-signing tagging', () => {
  it.each([ENDPOINTS.TIME, ENDPOINTS.MARKETS, ENDPOINTS.TICKERS, ENDPOINTS.MARK_PRICES])(
    'tags public endpoint %s',
    async (url) => {
      const config = await capture(buildXhr(LOCAL_KEYS), url);

      expect(isTagged(config)).toBe(true);
      expect(config.headers['X-Signature']).toBeUndefined();
    },
  );

  it('keeps private endpoints untagged in the hosted keyless shape', async () => {
    const config = await capture(buildXhr(HOSTED), ENDPOINTS.POSITIONS);

    expect(isTagged(config)).toBe(false);
  });
});
