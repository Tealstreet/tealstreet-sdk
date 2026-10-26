import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { describe, expect, it } from 'vitest';

import { createAPI } from '../reference/coinbase/coinbase.api';
import { ENDPOINTS } from '../reference/coinbase/coinbase.types';
import { normalizeCoinbaseApiCredentials, sign } from '../reference/coinbase/coinbase.utils';

const KEY_NAME = 'organizations/org-id/apiKeys/key-id';

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

const decodeJwtPart = <T>(value: string): T => JSON.parse(Buffer.from(value, 'base64url').toString('utf8')) as T;

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
  it('normalizes downloaded Advanced Trade JSON credentials', () => {
    const json = JSON.stringify({
      name: KEY_NAME,
      privateKey: '-----BEGIN EC PRIVATE KEY-----\\nabc\\n-----END EC PRIVATE KEY-----',
    });

    expect(normalizeCoinbaseApiCredentials(json, '')).toEqual({
      algorithm: 'ES256',
      keyName: KEY_NAME,
      keySecret: '-----BEGIN EC PRIVATE KEY-----\nabc\n-----END EC PRIVATE KEY-----',
    });
  });

  it('normalizes current Ed25519 Coinbase CDP key secrets', async () => {
    const seed = Uint8Array.from({ length: 32 }, (_, index) => index + 1);
    const secret = Buffer.concat([Buffer.from(seed), Buffer.alloc(32, 7)]).toString('base64');

    const normalized = normalizeCoinbaseApiCredentials(KEY_NAME, secret);
    expect(normalized).toMatchObject({
      algorithm: 'EdDSA',
      keyName: KEY_NAME,
      keySecret: secret,
    });
    expect(normalized.ed25519Seed).toEqual(seed);

    const token = await sign(KEY_NAME, secret, 'GET api.coinbase.com/api/v3/brokerage/products');
    const [encodedHeader, encodedPayload, encodedSignature] = token.split('.');

    expect(decodeJwtPart<Record<string, unknown>>(encodedHeader)).toMatchObject({ alg: 'EdDSA', kid: KEY_NAME });
    expect(decodeJwtPart<Record<string, unknown>>(encodedPayload)).toMatchObject({
      iss: 'cdp',
      aud: ['cdp_service'],
      sub: KEY_NAME,
      uri: 'GET api.coinbase.com/api/v3/brokerage/products',
    });
    expect(encodedSignature).toBeTruthy();
  });

  it('rejects Coinbase credentials before WebCrypto when the wrong field is pasted', () => {
    expect(() => normalizeCoinbaseApiCredentials('api-key-id-only', 'api-secret')).toThrow(
      'Coinbase API key must be the Advanced Trade CDP key name',
    );
    expect(() => normalizeCoinbaseApiCredentials('organizations/org-id/apiKeys/key-id', 'api-secret')).toThrow(
      'Coinbase API secret must be the Advanced Trade private key',
    );
  });

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
