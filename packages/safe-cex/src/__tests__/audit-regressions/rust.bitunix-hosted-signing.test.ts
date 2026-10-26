import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHash } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { createAPI } from '../../exchanges/bitunix/bitunix.api';
import { signHostedHttp, signHostedWsAuth } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';

vi.mock('../../utils/uuid', () => ({ uuid: () => 'nonce-abc' }));

const stamp = 1_709_209_849_123;
const doubleSha256 = (toSign: string) => createHash('sha256')
  .update(createHash('sha256').update(toSign).digest('hex') + 'fixture-material').digest('hex');
const order = (): RemoteSignerRequest => ({ id: 'capture', type: 'http', exchangeName: 'bitunix',
  config: { method: 'POST', url: '/api/v1/futures/trade/place_order', baseURL: 'https://fixture.invalid',
    params: { symbol: 'BTCUSDT', note: 'é/+%' }, data: { symbol: 'BTCUSDT', qty: '0.01', side: 'BUY' },
    headers: { 'X-Transport': 'keep', 'api-key': 'stale', sign: 'stale' } } });
const preimage = `nonce-abc${stamp}owned-publicnoteé/+%symbolBTCUSDT{"symbol":"BTCUSDT","qty":"0.01","side":"BUY"}`;

it('captures the exact Bitunix REST preimage with the presented key and no TS venue', async () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => input.kind === 'credential'
    ? { credential: 'owned-public' } : { credential: 'owned-public', signature: 'owned-hex-mac' });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const input = order();
  const before = structuredClone(input);
  const out = await signHostedHttp('bitunix', { key: '', secret: '' } as ExchangeOptions, true, host, input);
  expect(hostedSigning.mock.calls).toEqual([
    [{ kind: 'credential' }],
    [{ kind: 'http', stampMs: stamp, revision: '3', payload: preimage }],
  ]);
  expect(out.config).toMatchObject({ url: '/api/v1/futures/trade/place_order', headers: {
    'X-Transport': 'keep', 'api-key': 'owned-public', sign: 'owned-hex-mac', nonce: 'nonce-abc', timestamp: String(stamp),
  } });
  expect(input).toEqual(before);
});

it('leaves the legacy unsigned cases unsigned and refuses a changed provider identity', async () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => input.kind === 'credential'
    ? { credential: 'owned-public' } : { credential: 'rotated-public', signature: 'mac' });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  // The legacy interceptor skips only URLs containing `public`, and keyless accounts.
  for (const [url, primaryComplete] of [['/public/fixture', true], ['/api/v1/futures/market/tickers', false]] as const) {
    const out = await signHostedHttp('bitunix', { key: '', secret: '' }, primaryComplete, host, { id: 'unsigned', type: 'http',
      exchangeName: 'bitunix', config: { method: 'GET', baseURL: 'https://fixture.invalid', url, headers: {} } });
    expect(out.config?.headers).not.toHaveProperty('sign');
  }
  expect(hostedSigning).not.toHaveBeenCalled();
  await expect(signHostedHttp('bitunix', { key: '', secret: '' }, true, host, order())).rejects.toThrow('Signing identity changed');
});

it('hosted capture and the legacy TS interceptor share one wire for the same request', async () => {
  vi.spyOn(virtualClock, 'getCurrentTime').mockReturnValue(stamp as never);
  const legacy = await signHttpCapture(createAPI({ key: 'owned-public', secret: 'fixture-material' } as ExchangeOptions,
    { name: 'bitunix', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), order());
  const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }),
    hostedSigning: async (input: { kind: string; payload?: string }) => input.kind === 'credential' ? { credential: 'owned-public' }
      : { credential: 'owned-public', signature: doubleSha256(input.payload!) } } as unknown as EngineHost;
  const hosted = await signHostedHttp('bitunix', { key: '', secret: '' }, true, host, order());
  for (const header of ['api-key', 'sign', 'nonce', 'timestamp'])
    expect(String(hosted.config?.headers[header]), header).toBe(String(legacy.config?.headers[header]));
  expect(legacy.config?.headers.sign).toBe(doubleSha256(preimage));
  expect(hosted.config?.params).toEqual(legacy.config?.params);
  expect(hosted.config?.data).toBe(legacy.config?.data);
});

it('passes the bitunix-double-sha256 login format to the owned capture', async () => {
  const hostedSigning = vi.fn(async () => ({ signature: 'owned-login' }));
  const host = { hostedSigning } as unknown as EngineHost;
  await expect(signHostedWsAuth(host, { id: 'login', type: 'ws-auth', exchangeName: 'bitunix',
    wsAuth: { toSign: 'nonce-abc1700000000owned-public', outputFormat: 'bitunix-double-sha256', extraData: { nonce: 'ignored' } } }))
    .resolves.toEqual({ id: 'login', wsAuthSignature: 'owned-login' });
  expect(hostedSigning).toHaveBeenCalledWith({ kind: 'wsAuth', toSign: 'nonce-abc1700000000owned-public', outputFormat: 'bitunix-double-sha256' });
});
