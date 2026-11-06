import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { createAPI } from '../reference/blofin/blofin.api';
import { signHostedHttp } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';

const stamp = 1_709_209_849_123;
const owned = (input: { kind: string }) => input.kind === 'credential'
  ? { credential: 'owned-public' }
  : { credential: 'owned-public', signature: 'owned-mac', headers: [['ACCESS-PASSPHRASE', 'owned-passphrase']] };
// BloFin signs base64 of the HMAC's lowercase hex text.
const blofinMac = (payload: string) =>
  Buffer.from(createHmac('sha256', 'fixture-material').update(payload).digest('hex')).toString('base64');

it('captures the exact BloFin HTTP preimage, nonce and trusted passphrase without a TS venue', async () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => owned(input));
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const input: RemoteSignerRequest = { id: 'capture', type: 'http', exchangeName: 'blofin',
    config: { method: 'POST', url: '/api/v1/trade/order', baseURL: 'https://fixture.invalid',
      params: { note: 'é/+%' }, data: { instId: 'BTC-USDT', size: '0.0100' },
      headers: { 'X-Transport': 'keep', 'ACCESS-KEY': 'stale', 'ACCESS-PASSPHRASE': 'stale' } } };
  const before = structuredClone(input);
  const out = await signHostedHttp('blofin', { key: '', secret: '' } as ExchangeOptions, true, host, input);
  const nonce = out.config?.headers['ACCESS-NONCE'];
  expect(nonce).toMatch(/^\S{8,}$/);
  expect(hostedSigning.mock.calls).toEqual([
    [{ kind: 'credential' }],
    [{ kind: 'http', stampMs: stamp, revision: '3',
      payload: `/api/v1/trade/order?note=é/+%POST${stamp}${nonce}{"instId":"BTC-USDT","size":"0.0100"}` }],
  ]);
  expect(out.config).toMatchObject({ url: '/api/v1/trade/order', params: { note: 'é/+%' },
    data: '{"instId":"BTC-USDT","size":"0.0100"}', headers: {
      'Content-Type': 'application/json', 'X-Transport': 'keep', 'ACCESS-KEY': 'owned-public',
      'ACCESS-SIGN': 'owned-mac', 'ACCESS-TIMESTAMP': String(stamp), 'ACCESS-PASSPHRASE': 'owned-passphrase',
    } });
  expect(input).toEqual(before);
});

it('signs a replayed string body as sent and leaves public and market paths unsigned', async () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => owned(input));
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const replay = await signHostedHttp('blofin', { key: '', secret: '' }, true, host, {
    id: 'replay', type: 'http', exchangeName: 'blofin',
    config: { method: 'post', baseURL: 'https://fixture.invalid', url: '/api/v1/trade/cancel-order', data: '{"orderId":"1"}' },
  });
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', stampMs: stamp, revision: '3',
    payload: `/api/v1/trade/cancel-orderPOST${stamp}${replay.config?.headers['ACCESS-NONCE']}{"orderId":"1"}` });
  hostedSigning.mockClear();
  for (const url of ['/api/v1/market/tickers', '/api/v1/public/instruments']) {
    const out = await signHostedHttp('blofin', { key: '', secret: '' }, true, host, {
      id: 'public', type: 'http', exchangeName: 'blofin', config: { method: 'GET', baseURL: 'https://fixture.invalid', url },
    });
    expect(out.config?.headers).not.toHaveProperty('ACCESS-SIGN');
  }
  expect(hostedSigning).not.toHaveBeenCalled();
});

it('without a primary identity returns the request unsigned without provider custody', async () => {
  const hostedSigning = vi.fn().mockRejectedValue(new Error('must not call'));
  const host = { hostedSigning, hostedSigningClock: vi.fn() } as unknown as EngineHost;
  const out = await signHostedHttp('blofin', { key: '', secret: '' }, false, host, {
    id: 'keyless', type: 'http', exchangeName: 'blofin',
    config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/api/v1/account/balance', headers: {} },
  });
  expect(out.error).toBeUndefined();
  expect(out.config?.headers).not.toHaveProperty('ACCESS-SIGN');
  expect(hostedSigning).not.toHaveBeenCalled();
});

it('refuses a missing provider passphrase or a rotated credential before returning a signed config', async () => {
  for (const answer of [
    { credential: 'owned-public', signature: 'must-not-publish' },
    { credential: 'rotated-public', signature: 'mac', headers: [['ACCESS-PASSPHRASE', 'presentation']] },
  ]) {
    const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }),
      hostedSigning: async (input: { kind: string }) => input.kind === 'credential' ? { credential: 'owned-public' } : answer,
    } as unknown as EngineHost;
    await expect(signHostedHttp('blofin', { key: '', secret: '' }, true, host, {
      id: 'refused', type: 'http', exchangeName: 'blofin',
      config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/api/v1/account/balance' },
    })).rejects.toThrow('Signing identity changed');
  }
});

it('hosted capture and the legacy TS interceptor share one wire for the same request', async () => {
  vi.spyOn(virtualClock, 'getCurrentTime').mockReturnValue({ valueOf: () => stamp } as never);
  const request = (): RemoteSignerRequest => ({ id: 'parity', type: 'http', exchangeName: 'blofin',
    config: { method: 'POST', url: '/api/v1/trade/order', baseURL: 'https://fixture.invalid',
      params: { instId: 'BTC-USDT' }, data: { instId: 'BTC-USDT', side: 'buy', size: '1' } } });
  const legacy = await signHttpCapture(createAPI({ key: 'fixture-public', secret: 'fixture-material', passphrase: 'fixture-pass' } as ExchangeOptions,
    { name: 'blofin', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), request());
  const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }),
    hostedSigning: async (input: { kind: string; payload?: string }) => input.kind === 'credential' ? { credential: 'fixture-public' }
      : { credential: 'fixture-public', signature: blofinMac(input.payload!), headers: [['ACCESS-PASSPHRASE', 'fixture-pass']] } } as unknown as EngineHost;
  const hosted = await signHostedHttp('blofin', { key: '', secret: '' }, true, host, request());
  for (const header of ['ACCESS-KEY', 'ACCESS-TIMESTAMP', 'ACCESS-PASSPHRASE'])
    expect(hosted.config?.headers[header], header).toBe(legacy.config?.headers[header]);
  // Each side draws its own nonce; both sign the same grammar over it.
  for (const signed of [legacy, hosted]) {
    const nonce = signed.config?.headers['ACCESS-NONCE'];
    expect(signed.config?.headers['ACCESS-SIGN']).toBe(
      blofinMac(`/api/v1/trade/order?instId=BTC-USDTPOST${stamp}${nonce}{"instId":"BTC-USDT","side":"buy","size":"1"}`));
  }
  expect(hosted.config?.params).toEqual(legacy.config?.params);
  expect(hosted.config?.data).toBe(legacy.config?.data);
});
