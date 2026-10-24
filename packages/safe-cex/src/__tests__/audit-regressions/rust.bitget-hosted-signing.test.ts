import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { createAPI } from '../../exchanges/bitget/bitget.api';
import { signHostedHttp } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';

const aliases = ['bitget', 'bitgetusdc', 'bitgetinverse'] as const;
const stamp = 1_709_209_849_123;
const owned = (input: { kind: string }) => input.kind === 'credential'
  ? { credential: 'owned-public' }
  : { credential: 'owned-public', signature: 'owned-base64-mac', headers: [['ACCESS-PASSPHRASE', 'owned-passphrase']] };

it.each(aliases)('%s captures the exact Bitget HTTP preimage and trusted passphrase without a TS venue', async (name) => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => owned(input));
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const input: RemoteSignerRequest = { id: 'capture', type: 'http', exchangeName: name,
    config: { method: 'POST', url: '/api/v2/mix/order/place-order', baseURL: 'https://fixture.invalid',
      params: { symbol: 'BTCUSDT$', note: 'é/+%' }, data: { symbol: 'BTCUSDT$', _size: '0.0100', memo: 'é/+%' },
      headers: { 'X-Transport': 'keep', 'ACCESS-KEY': 'stale', 'ACCESS-PASSPHRASE': 'stale' } } };
  const before = structuredClone(input);
  const out = await signHostedHttp(name, { key: '', secret: '' } as ExchangeOptions, true, host, input);
  expect(hostedSigning.mock.calls).toEqual([
    [{ kind: 'credential' }],
    [{ kind: 'http', stampMs: stamp, revision: '3',
      payload: `${stamp}POST/api/v2/mix/order/place-order?symbol=BTCUSDT&note=é/+%{"symbol":"BTCUSDT","size":"0.0100","memo":"é/+%"}` }],
  ]);
  // The legacy interceptor signs the stripped body while axios sends the original.
  expect(out.config).toMatchObject({ url: '/api/v2/mix/order/place-order',
    params: { symbol: 'BTCUSDT', note: 'é/+%' }, data: '{"symbol":"BTCUSDT$","_size":"0.0100","memo":"é/+%"}', headers: {
      'X-CHANNEL-API-CODE': 'Tealstreet', 'Content-Type': 'application/json', 'X-Transport': 'keep',
      'ACCESS-KEY': 'owned-public', 'ACCESS-SIGN': 'owned-base64-mac', 'ACCESS-TIMESTAMP': String(stamp),
      'ACCESS-PASSPHRASE': 'owned-passphrase',
    } });
  expect(input).toEqual(before);
});

it.each(aliases)('%s keeps the candle symbol and replayed string bodies exactly as the legacy interceptor', async (name) => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => owned(input));
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  await signHostedHttp(name, { key: '', secret: '' }, true, host, {
    id: 'candles', type: 'http', exchangeName: name,
    config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/api/v2/mix/market/candles', params: { symbol: 'BTCUSDT$' } },
  });
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', stampMs: stamp, revision: '3',
    payload: `${stamp}GET/api/v2/mix/market/candles?symbol=BTCUSDT$` });
  await signHostedHttp(name, { key: '', secret: '' }, true, host, {
    id: 'replay', type: 'http', exchangeName: name,
    config: { method: 'post', baseURL: 'https://fixture.invalid', url: '/api/v2/mix/order/cancel-order', data: '{"_b":2,"a":"x$"}' },
  });
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', stampMs: stamp, revision: '3',
    payload: `${stamp}POST/api/v2/mix/order/cancel-order{"b":2,"a":"x$"}` });
});

it.each(aliases)('%s without a primary identity returns the request unsigned without provider custody', async (name) => {
  const hostedSigning = vi.fn().mockRejectedValue(new Error('must not call'));
  const host = { hostedSigning, hostedSigningClock: vi.fn() } as unknown as EngineHost;
  const out = await signHostedHttp(name, { key: '', secret: '' }, false, host, {
    id: 'public', type: 'http', exchangeName: name,
    config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/api/v2/mix/market/contracts', params: { productType: 'usdt-futures' }, headers: {} },
  });
  expect(out.error).toBeUndefined();
  expect(out.config?.params).toEqual({ productType: 'usdt-futures' });
  expect(out.config?.headers).not.toHaveProperty('ACCESS-SIGN');
  expect(hostedSigning).not.toHaveBeenCalled();
});

it.each(aliases)('%s refuses a missing provider passphrase before returning any signed config', async (name) => {
  const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }),
    hostedSigning: async (input: { kind: string }) => input.kind === 'credential'
      ? { credential: 'owned-public' } : { credential: 'owned-public', signature: 'must-not-publish' } } as unknown as EngineHost;
  await expect(signHostedHttp(name, { key: '', secret: '' }, true, host, {
    id: 'missing', type: 'http', exchangeName: name,
    config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/api/v2/mix/account/accounts' },
  })).rejects.toThrow('Signing identity changed');
});

it.each(aliases)('%s refuses a provider credential that differs from the captured identity', async (name) => {
  const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }),
    hostedSigning: async (input: { kind: string }) => input.kind === 'credential'
      ? { credential: 'owned-public' }
      : { credential: 'rotated-public', signature: 'mac', headers: [['ACCESS-PASSPHRASE', 'presentation']] } } as unknown as EngineHost;
  await expect(signHostedHttp(name, { key: '', secret: '' }, true, host, {
    id: 'rotated', type: 'http', exchangeName: name,
    config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/api/v2/mix/account/accounts' },
  })).rejects.toThrow('Signing identity changed');
});

it.each(aliases)('%s hosted capture and the legacy TS interceptor share one wire for the same request', async (name) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  const request = (): RemoteSignerRequest => ({ id: 'parity', type: 'http', exchangeName: name,
    config: { method: 'POST', url: '/api/v2/mix/order/place-order', baseURL: 'https://fixture.invalid',
      params: { symbol: 'BTCUSDT$', productType: 'USDT-FUTURES' }, data: { symbol: 'BTCUSDT$', _size: '1', side: 'buy' } } });
  const mac = (payload: string) => createHmac('sha256', 'fixture-material').update(payload).digest('base64');
  const legacy = await signHttpCapture(createAPI({ key: 'fixture-public', secret: 'fixture-material', passphrase: 'fixture-pass' } as ExchangeOptions,
    { name, grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), request());
  const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }),
    hostedSigning: async (input: { kind: string; payload?: string }) => input.kind === 'credential' ? { credential: 'fixture-public' }
      : { credential: 'fixture-public', signature: mac(input.payload!), headers: [['ACCESS-PASSPHRASE', 'fixture-pass']] } } as unknown as EngineHost;
  const hosted = await signHostedHttp(name, { key: '', secret: '' }, true, host, request());
  for (const header of ['ACCESS-KEY', 'ACCESS-SIGN', 'ACCESS-TIMESTAMP', 'ACCESS-PASSPHRASE', 'X-CHANNEL-API-CODE', 'Content-Type'])
    expect(hosted.config?.headers[header], header).toBe(legacy.config?.headers[header]);
  expect(legacy.config?.headers['ACCESS-SIGN']).toBe(mac(`${stamp}POST/api/v2/mix/order/place-order?symbol=BTCUSDT&productType=USDT-FUTURES{"symbol":"BTCUSDT","size":"1","side":"buy"}`));
  expect(hosted.config?.params).toEqual(legacy.config?.params);
  expect(hosted.config?.data).toBe(legacy.config?.data);
});
