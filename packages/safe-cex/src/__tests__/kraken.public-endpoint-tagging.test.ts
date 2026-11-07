import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createAPI } from './reference/kraken/kraken.api';

// Drives the real request interceptor and captures the config it produces,
// rather than asserting on the tag helper in isolation. The interceptor is
// where both failure modes live: forgetting to tag a public endpoint, and
// tagging a private one because the condition was credential-shaped.
const buildXhr = (options: Record<string, unknown>): AxiosInstance =>
  createAPI({ testnet: false, ...options } as any, {
    name: 'kraken',
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
    emitter: { emit: () => undefined },
  } as any);

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

const LOCAL_KEYS = { key: 'k', secret: Buffer.from('s').toString('base64') };
// What the hosted exchange-server actually holds: the key, but no secret, plus
// a signer pointing back at the device.
const HOSTED = { key: 'k', secret: '', remoteSigner: { sign: async () => ({}), accountName: 'K' } };

describe('kraken public endpoint tagging', () => {
  it('tags a public endpoint', async () => {
    expect(isTagged(await capture(buildXhr(LOCAL_KEYS), '/v3/instruments'))).toBe(true);
  });

  it('leaves a public endpoint unsigned, as it always has been', async () => {
    const config = await capture(buildXhr(LOCAL_KEYS), '/v3/tickers');

    expect(config.headers.Authent).toBeUndefined();
    expect(config.headers.APIKey).toBeUndefined();
  });

  it('still signs a private endpoint locally when the secret is present', async () => {
    const config = await capture(buildXhr(LOCAL_KEYS), '/v3/openorders');

    expect(config.headers.Authent).toBeDefined();
    expect(isTagged(config)).toBe(false);
  });

  // The hazard the tag placement exists to avoid. The hosted server is handed
  // the key but never the secret, so `!options.secret` is true for every
  // request it makes; tagging on a guard that includes it would mark real
  // private requests as needing no signature, and the server would ship them
  // to Kraken unauthenticated.
  it('never tags a private endpoint in the hosted keyless shape', async () => {
    for (const url of ['/v3/openorders', '/v3/openpositions', '/v3/accounts', '/v3/sendorder']) {
      expect(isTagged(await capture(buildXhr(HOSTED), url))).toBe(false);
    }
  });

  it('still tags public endpoints in the hosted keyless shape', async () => {
    expect(isTagged(await capture(buildXhr(HOSTED), '/v3/instruments'))).toBe(true);
  });
});
