import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';
import type { PayloadSignerRequest } from '../types';

import createHmac from 'create-hmac';
import { describe, expect, it, vi } from 'vitest';

import { createAPI as createBinanceAPI } from './reference/binance/binance.api';
import { createPAPI as createBinancePmAPI } from './reference/binance/binancepm.api';
import { createAPI as createBinanceSpotAPI } from './reference/binance/binancespot.api';
import { createAPI as createBybitAPI } from './reference/bybit/bybit.api';
import { virtualClock } from '../utils/virtualClock';

const stubExchange = (name: string) =>
  ({
    name,
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
    emitter: { emit: () => undefined },
    recordAuthSuccess: () => undefined,
    recordAuthFailure: () => undefined,
  }) as any;

const capture = async (xhr: AxiosInstance, request: { method: 'get' | 'post'; url: string; params?: any; data?: any }) => {
  let captured: InternalAxiosRequestConfig | undefined;
  await xhr.request({
    ...request,
    adapter: (config: any) => {
      captured = config;
      return Promise.resolve({ data: { code: 0, retCode: 0 }, status: 200, statusText: 'OK', headers: {}, config });
    },
  });
  return captured!;
};

const recordingSigner = () => {
  const calls: PayloadSignerRequest[] = [];
  const signer = (request: PayloadSignerRequest) => {
    calls.push(request);
    return { signature: 'signature-from-signer', headers: { 'X-Signer-Header': Buffer.from(request.payload).toString('base64') } };
  };
  return { calls, signer };
};

const sentQuery = (config: InternalAxiosRequestConfig) =>
  (config.paramsSerializer as { serialize: (params: unknown) => string }).serialize(config.params);

describe('payloadSigner on bybit', () => {
  const bybit = (options: Record<string, unknown>) =>
    createBybitAPI({ testnet: false, key: 'api-key', ...options } as any, stubExchange('bybit'));

  it('signs timestamp + key + recvWindow + query and puts the result in X-BAPI-SIGN', async () => {
    const { calls, signer } = recordingSigner();
    const config = await capture(bybit({ payloadSigner: signer }), {
      method: 'get',
      url: '/v5/position/list',
      params: { category: 'linear' },
    });

    expect(calls).toHaveLength(1);
    const ts = config.headers['X-BAPI-TIMESTAMP'];
    expect(calls[0]).toMatchObject({ exchangeName: 'bybit', key: 'api-key', scheme: 'hmac-sha256', encoding: 'hex' });
    expect(calls[0].payload).toBe(`${ts}api-key${config.headers['X-BAPI-RECV-WINDOW']}${sentQuery(config)}`);
    expect(config.headers['X-BAPI-SIGN']).toBe('signature-from-signer');
    expect(config.headers['X-Signer-Header']).toBe(Buffer.from(calls[0].payload).toString('base64'));
  });

  it('signs the POST body exactly as sent', async () => {
    const { calls, signer } = recordingSigner();
    const config = await capture(bybit({ payloadSigner: signer }), {
      method: 'post',
      url: '/v5/order/create',
      data: { category: 'linear', symbol: 'BTCUSDT' },
    });

    expect(calls[0].payload.endsWith(config.data)).toBe(true);
  });

  it('never calls the signer for public market data', async () => {
    const { calls, signer } = recordingSigner();
    await capture(bybit({ payloadSigner: signer }), { method: 'get', url: '/v5/market/kline', params: { symbol: 'BTCUSDT' } });

    expect(calls).toHaveLength(0);
  });

  it('still signs locally with a secret', async () => {
    const config = await capture(bybit({ secret: 's' }), {
      method: 'get',
      url: '/v5/position/list',
      params: { category: 'linear' },
    });
    const payload = `${config.headers['X-BAPI-TIMESTAMP']}api-key${config.headers['X-BAPI-RECV-WINDOW']}${sentQuery(config)}`;

    expect(config.headers['X-BAPI-SIGN']).toBe(createHmac('sha256', 's').update(payload).digest('hex'));
  });
});

describe('payloadSigner on binance', () => {
  it.each([
    ['binance', createBinanceAPI, '/fapi/v3/balance'],
    ['binancespot', createBinanceSpotAPI, '/api/v3/account'],
    ['binancepm', createBinancePmAPI, '/papi/v1/balance'],
  ] as const)('%s signs the query it sends and appends the signature', async (name, createAPI, url) => {
    const { calls, signer } = recordingSigner();
    const xhr = createAPI({ testnet: false, key: 'api-key', payloadSigner: signer } as any, stubExchange(name));
    const config = await capture(xhr, { method: 'post', url, data: { symbol: 'BTCUSDT' } });

    expect(calls).toHaveLength(1);
    expect(config.data).toBeUndefined();
    expect(sentQuery(config)).toBe(`${calls[0].payload}&signature=signature-from-signer`);
    expect(config.headers['X-Signer-Header']).toBe(Buffer.from(calls[0].payload).toString('base64'));
  });

  it('drops a previous signature before signing again', async () => {
    const { calls, signer } = recordingSigner();
    const xhr = createBinanceAPI({ testnet: false, key: 'api-key', payloadSigner: signer } as any, stubExchange('binance'));
    await capture(xhr, { method: 'get', url: '/fapi/v3/balance', params: { signature: 'stale' } });

    expect(calls[0].payload).not.toContain('signature=');
  });

  it('still signs locally with a secret', async () => {
    const xhr = createBinanceAPI({ testnet: false, key: 'api-key', secret: 's' } as any, stubExchange('binance'));
    const config = await capture(xhr, { method: 'get', url: '/fapi/v3/balance' });
    const { signature, ...rest } = config.params;

    expect(signature).toBe(createHmac('sha256', 's').update(sentQuery({ ...config, params: rest })).digest('hex'));
  });
});

// A resend replays the config the first attempt went out with, its signer headers
// included; the replay's signer headers must be the ones made for the replay.
describe('payloadSigner headers on a replayed request', () => {
  it.each([
    ['bybit', (signer: any) => createBybitAPI({ testnet: false, key: 'api-key', payloadSigner: signer } as any, stubExchange('bybit')), '/v5/position/list'],
    ['binance', (signer: any) => createBinanceAPI({ testnet: false, key: 'api-key', payloadSigner: signer } as any, stubExchange('binance')), '/fapi/v3/balance'],
  ])('%s', async (_, create, url) => {
    let now = 1_800_000_000_000;
    const clock = vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => (now += 1_000));
    const { calls, signer } = recordingSigner();
    const xhr = create(signer);

    const first = await capture(xhr, { method: 'get', url, params: { a: '1' } });
    const replay = await capture(xhr, first as any);

    expect(calls).toHaveLength(2);
    expect(calls[1].payload).not.toBe(calls[0].payload);
    expect(replay.headers['X-Signer-Header']).toBe(Buffer.from(calls[1].payload).toString('base64'));
    clock.mockRestore();
  });
});
