import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createExchangeAPI, createInfoAPI } from './reference/hyperliquid/hyperliquid.api';
import { createAPI as createSpotAPI } from './reference/hyperliquid/hyperliquidspot.api';

// Well-known test key; no value, never used in production.
const KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';

const stubExchange = () =>
  ({
    name: 'hyperliquid',
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
    emitter: { emit: () => undefined },
    emitAndLogError: () => undefined,
  }) as any;

const capture = async (xhr: AxiosInstance, url: string, data?: unknown) => {
  let captured: InternalAxiosRequestConfig | undefined;
  await xhr.request({
    method: 'post',
    url,
    data,
    adapter: (config: any) => {
      captured = config;
      return Promise.resolve({ data: { status: 'ok' }, status: 200, statusText: 'OK', headers: {}, config });
    },
  });
  return captured!;
};

const isTagged = (config: InternalAxiosRequestConfig) =>
  (config as unknown as Record<string, unknown>).safeCexSkipRemoteSigning === true;

const LOCAL_KEYS = { secret: KEY, key: '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266' };
// The hosted exchange-server holds no secret and signs via the device — which is
// exactly why the `!options.secret` guard is the wrong place to tag.
const HOSTED = { ...LOCAL_KEYS, secret: '', remoteSigner: { sign: async () => ({}), accountName: 'H' } };

const ORDER = {
  action: { type: 'order', orders: [], grouping: 'na' },
  nonce: 1,
};

describe('hyperliquid /info tagging', () => {
  it.each([
    ['exchange api', createExchangeAPI],
    ['info api', createInfoAPI],
  ])('tags /info on the %s', async (_label, create) => {
    const xhr = create({ testnet: false, ...LOCAL_KEYS } as any, stubExchange());

    expect(isTagged(await capture(xhr, '/info', { type: 'userFills' }))).toBe(true);
  });

  // The production path. `createExchangeAPI`'s tag sits above the
  // `!options.secret` guard precisely so it still runs here.
  it.each([
    ['exchange api', createExchangeAPI],
    ['info api', createInfoAPI],
  ])('tags /info on the %s in the hosted keyless shape', async (_label, create) => {
    const xhr = create({ testnet: false, ...HOSTED } as any, stubExchange());

    expect(isTagged(await capture(xhr, '/info', { type: 'userFills' }))).toBe(true);
  });

  // The hazard: /exchange carries signable actions. If it were ever tagged, a
  // hosted order would be shipped to Hyperliquid with no signature at all.
  it.each([
    ['exchange api', createExchangeAPI],
    ['info api', createInfoAPI],
  ])('never tags an /exchange order on the %s', async (_label, create) => {
    for (const options of [LOCAL_KEYS, HOSTED]) {
      const xhr = create({ testnet: false, ...options } as any, stubExchange());

      expect(isTagged(await capture(xhr, '/exchange', ORDER))).toBe(false);
    }
  });

  it('still signs an /exchange order locally when the secret is present', async () => {
    const xhr = createExchangeAPI({ testnet: false, ...LOCAL_KEYS } as any, stubExchange());

    const config = await capture(xhr, '/exchange', ORDER);

    // axios' transformRequest has already stringified the body by adapter time.
    const body = typeof config.data === 'string' ? JSON.parse(config.data) : config.data;
    expect(body.signature).toBeDefined();
  });
});

describe('hyperliquidspot /info tagging', () => {
  it('tags /info and never /exchange', async () => {
    const xhr = createSpotAPI({ testnet: false, ...HOSTED } as any, stubExchange());

    expect(isTagged(await capture(xhr, '/info', { type: 'spotClearinghouseState' }))).toBe(true);
    expect(isTagged(await capture(xhr, '/exchange', ORDER))).toBe(false);
  });
});
