import { describe, expect, it } from 'vitest';

import { createAPI } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.api';

import { readConfig } from './config';

const signerEnv = {
  EXCHANGE: 'bybit',
  EXCHANGE_API_KEY: 'api-key',
  EXCHANGE_SIGNER_HEADER: 'X-Sign-Me',
  EXCHANGE_SIGNER_SIGNATURE: 'placeholder',
};

const stubExchange = {
  name: 'bybit',
  grafanaEndpointsEvery: [],
  grafanaEndpointsPeriodic: [],
  emitter: { emit: () => undefined },
  recordAuthSuccess: () => undefined,
  recordAuthFailure: () => undefined,
} as any;

describe('readConfig', () => {
  it('sends a Bybit request a signing proxy can complete: the placeholder as the signature, the signed bytes in the header', async () => {
    const { options } = readConfig(signerEnv);
    let sent: any;
    await createAPI(options, stubExchange).request({
      method: 'get',
      url: '/v5/position/list',
      params: { category: 'linear' },
      adapter: (config: any) => {
        sent = config;
        return Promise.resolve({ data: { retCode: 0 }, status: 200, statusText: 'OK', headers: {}, config });
      },
    });

    const signed = Buffer.from(sent.headers['X-Sign-Me'], 'base64').toString('utf8');
    expect(sent.headers['X-BAPI-SIGN']).toBe('placeholder');
    expect(signed.startsWith(`${sent.headers['X-BAPI-TIMESTAMP']}api-key`)).toBe(true);
    expect(signed).toContain('category=linear');
  });

  it('turns options on only when asked', () => {
    expect(readConfig({ ...signerEnv, EXCHANGE_OPTIONS: 'true' }).options.enableOptionMarkets).toBe(true);
    expect(readConfig(signerEnv).options.enableOptionMarkets).toBe(false);
  });

  it('turns options on for an exchange that trades only options, with or without EXCHANGE_OPTIONS', () => {
    expect(readConfig({ EXCHANGE: 'binanceoptions' }).options.enableOptionMarkets).toBe(true);
    expect(readConfig({ EXCHANGE: 'binanceoptions', EXCHANGE_OPTIONS: 'true' }).options.enableOptionMarkets).toBe(true);
  });

  it('turns inverse on only when asked', () => {
    expect(readConfig({ ...signerEnv, EXCHANGE_INVERSE: 'true' }).options.enableInverseMarkets).toBe(true);
    expect(readConfig(signerEnv).options.enableInverseMarkets).toBe(false);
  });

  it('turns polling off', () => {
    expect(readConfig(signerEnv).options.extra?.tickInterval).toBe(Number.POSITIVE_INFINITY);
  });

  it.each([
    [{ EXCHANGE: 'nope' }, 'EXCHANGE must be one of'],
    [{ ...signerEnv, EXCHANGE_SIGNER_SIGNATURE: '' }, 'must be set together'],
    [{ ...signerEnv, EXCHANGE_API_SECRET: 'secret' }, 'set one'],
    [{ EXCHANGE: 'bybit', EXCHANGE_TESTNET: 'yes' }, 'EXCHANGE_TESTNET'],
    [{ EXCHANGE: 'bybit', EXCHANGE_OPTIONS: 'yes' }, 'EXCHANGE_OPTIONS must be true or false'],
    [{ EXCHANGE: 'bybitspot', EXCHANGE_OPTIONS: 'true' }, 'EXCHANGE_OPTIONS is not available on bybitspot'],
    [{ EXCHANGE: 'bybit', EXCHANGE_INVERSE: '1' }, 'EXCHANGE_INVERSE must be true or false'],
    [{ EXCHANGE: 'bybitinverse', EXCHANGE_INVERSE: 'true' }, 'EXCHANGE_INVERSE is not available on bybitinverse'],
    [{ EXCHANGE: 'bybitdemo', EXCHANGE_INVERSE: 'true' }, 'EXCHANGE_INVERSE is not available on bybitdemo'],
    [{ EXCHANGE: 'hyrotrader', EXCHANGE_INVERSE: 'true' }, 'EXCHANGE_INVERSE is not available on hyrotrader'],
  ])('refuses %j', (env, message) => {
    expect(() => readConfig(env)).toThrow(message);
  });
});
