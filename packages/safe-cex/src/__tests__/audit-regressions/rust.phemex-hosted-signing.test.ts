import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { createAPI } from '../reference/phemex/phemex.api';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { signHostedHttp } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';

const stamp = 1_709_209_849_123;
const expiry = '1709209909';
const hmacHex = (toSign: string) => createHmac('sha256', 'fixture-material').update(toSign).digest('hex');
const order = (): RemoteSignerRequest => ({ id: 'capture', type: 'http', exchangeName: 'phemex',
  config: { method: 'POST', url: '/g-orders/create', baseURL: 'https://fixture.invalid',
    params: { symbol: 'BTCUSDT', note: 'é/+%' }, data: { symbol: 'BTCUSDT', orderQtyRq: '0.01', side: 'Buy' },
    headers: { 'X-Transport': 'keep', 'X-PHEMEX-ACCESS-TOKEN': 'stale', 'X-PHEMEX-REQUEST-SIGNATURE': 'stale' } } });
const preimage = `/g-orders/createsymbol=BTCUSDT&note=é/+%${expiry}{"symbol":"BTCUSDT","orderQtyRq":"0.01","side":"Buy"}`;

it('declares RemoteSigner with the expiry header protected and no private PayloadSigner', () => {
  expect(deriveRemoteSigningPolicy('phemex')).toEqual({ enabled: true, payloadSupported: false,
    protectedHeaders: ['X-PHEMEX-ACCESS-TOKEN', 'X-PHEMEX-REQUEST-SIGNATURE', 'X-PHEMEX-REQUEST-EXPIRY'] });
  expect(deriveBridgeConstruction('phemex', {} as ExchangeOptions).dispatchSigning).toBe(true);
});

it('captures the exact Phemex REST preimage with the presented key and no TS venue', async () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => input.kind === 'credential'
    ? { credential: 'owned-public' } : { credential: 'owned-public', signature: 'owned-hex-mac' });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const input = order();
  const before = structuredClone(input);
  const out = await signHostedHttp('phemex', { key: '', secret: '' } as ExchangeOptions, true, host, input);
  expect(hostedSigning.mock.calls).toEqual([
    [{ kind: 'credential' }],
    [{ kind: 'http', stampMs: stamp, revision: '3', payload: preimage }],
  ]);
  expect(out.config).toMatchObject({ url: '/g-orders/create', params: input.config!.params, headers: {
    'X-Transport': 'keep', 'Content-Type': 'application/json', 'X-PHEMEX-ACCESS-TOKEN': 'owned-public',
    'X-PHEMEX-REQUEST-EXPIRY': expiry, 'X-PHEMEX-REQUEST-SIGNATURE': 'owned-hex-mac',
  } });
  expect(input).toEqual(before);
});

it('leaves keyless captures unsigned and refuses a changed provider identity', async () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => input.kind === 'credential'
    ? { credential: 'owned-public' } : { credential: 'rotated-public', signature: 'mac' });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const out = await signHostedHttp('phemex', { key: '', secret: '' }, false, host, { id: 'unsigned', type: 'http',
    exchangeName: 'phemex', config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/public/products', headers: {} } });
  expect(out.config?.headers).not.toHaveProperty('X-PHEMEX-REQUEST-SIGNATURE');
  expect(hostedSigning).not.toHaveBeenCalled();
  await expect(signHostedHttp('phemex', { key: '', secret: '' }, true, host, order())).rejects.toThrow('Signing identity changed');
});

it.each([
  ['an order body', order()],
  ['a bodiless read', { id: 'read', type: 'http', exchangeName: 'phemex', config: { method: 'GET',
    url: '/g-accounts/accountPositions', baseURL: 'https://fixture.invalid', params: { currency: 'USDT' }, headers: {} } }],
] as [string, RemoteSignerRequest][])('hosted capture and the TS reference signer share one wire for %s', async (_, request) => {
  vi.spyOn(virtualClock, 'getCurrentTime').mockReturnValue(stamp as never);
  const legacy = await signHttpCapture(createAPI({ key: 'owned-public', secret: 'fixture-material' } as ExchangeOptions,
    { name: 'phemex', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), request);
  const host = { hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }),
    hostedSigning: async (input: { kind: string; payload?: string }) => input.kind === 'credential' ? { credential: 'owned-public' }
      : { credential: 'owned-public', signature: hmacHex(input.payload!) } } as unknown as EngineHost;
  const hosted = await signHostedHttp('phemex', { key: '', secret: '' }, true, host, request);
  for (const header of ['Content-Type', 'X-PHEMEX-ACCESS-TOKEN', 'X-PHEMEX-REQUEST-EXPIRY', 'X-PHEMEX-REQUEST-SIGNATURE'])
    expect(String(hosted.config?.headers[header]), header).toBe(String(legacy.config?.headers[header]));
  expect(legacy.config?.headers['X-PHEMEX-REQUEST-EXPIRY']).toBe(expiry);
  if (request.id === 'capture') expect(legacy.config?.headers['X-PHEMEX-REQUEST-SIGNATURE']).toBe(hmacHex(preimage));
  expect(hosted.config?.params).toEqual(legacy.config?.params);
  expect(hosted.config?.data).toBe(legacy.config?.data);
});
