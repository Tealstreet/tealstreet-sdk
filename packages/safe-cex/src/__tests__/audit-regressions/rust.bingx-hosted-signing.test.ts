import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { createAPI } from '../reference/bingx/bingx.api';
import { signHostedHttp, signHostedWsAuth } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';

const stamp = 1_709_209_849_123;
const hmac = (payload: string) => createHmac('sha256', 'fixture-material').update(payload).digest('hex');
const place = (): RemoteSignerRequest => ({ id: 'place', type: 'http', exchangeName: 'bingx',
  config: { method: 'POST', url: '/openApi/swap/v2/trade/order', baseURL: 'https://fixture.invalid',
    data: { symbol: 'BTC-USDT', type: 'LIMIT', side: 'BUY', positionSide: 'BOTH', quantity: '0.012', price: '90000',
      clientOrderId: 'caller_bingx-01', signature: 'stale' },
    headers: { 'X-Transport': 'keep' } } });
const cancel = (): RemoteSignerRequest => ({ id: 'cancel', type: 'http', exchangeName: 'bingx',
  config: { method: 'DELETE', url: '/openApi/swap/v2/trade/order', baseURL: 'https://fixture.invalid',
    params: { symbol: 'BTC-USDT', orderId: '1989246372640980993' }, headers: {} } });
const amend = (): RemoteSignerRequest => ({ id: 'amend', type: 'http', exchangeName: 'bingx',
  config: { method: 'POST', url: '/openApi/swap/v1/trade/amend', baseURL: 'https://fixture.invalid',
    data: { symbol: 'BTC-USDT', orderId: '1989246372640980993', quantity: '0.02' }, headers: {} } });
const placePreimage = `clientOrderId=caller_bingx-01&positionSide=BOTH&price=90000&quantity=0.012&side=BUY&symbol=BTC-USDT&timestamp=${stamp}&type=LIMIT`;
const owned = (signature: (payload: string) => string, credential = 'owned-public') => {
  const hostedSigning = vi.fn(async (input: { kind: string; payload?: string }) => input.kind === 'credential'
    ? { credential: 'owned-public' } : { credential, signature: signature(input.payload!) });
  return { hostedSigning, host: { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost };
};

it('captures the exact BingX preimage with the presented key and no TS venue', async () => {
  const { hostedSigning, host } = owned(() => 'owned-hex-mac');
  // A stale key header never survives: the key is presented from custody.
  const input = place();
  input.config!.headers!['X-BX-APIKEY'] = 'stale';
  const before = structuredClone(input);
  const out = await signHostedHttp('bingx', { key: '', secret: '' } as ExchangeOptions, true, host, input);
  expect(hostedSigning.mock.calls).toEqual([
    [{ kind: 'credential' }],
    [{ kind: 'http', stampMs: stamp, revision: '3', payload: placePreimage }],
  ]);
  expect(out.config?.headers).toMatchObject({ 'X-Transport': 'keep', 'X-BX-APIKEY': 'owned-public', 'Content-Type': 'application/json' });
  expect(JSON.parse(String(out.config!.data))).toEqual({ clientOrderId: 'caller_bingx-01', positionSide: 'BOTH', price: 90000,
    quantity: 0.012, side: 'BUY', symbol: 'BTC-USDT', timestamp: stamp, type: 'LIMIT', signature: 'owned-hex-mac' });
  expect(input).toEqual(before);
});

it('leaves the livenet listen key unsigned and refuses a changed provider identity', async () => {
  const { hostedSigning, host } = owned(() => 'mac', 'rotated-public');
  const listenKey: RemoteSignerRequest = { id: 'listen', type: 'http', exchangeName: 'bingx',
    config: { method: 'POST', baseURL: 'https://fixture.invalid', url: '/openApi/user/auth/userDataStream', headers: {} } };
  const out = await signHostedHttp('bingx', { key: '', secret: '' }, true, host, listenKey);
  expect(out.config?.params).toBeUndefined();
  expect(out.config?.data).toBeUndefined();
  expect(hostedSigning).not.toHaveBeenCalled();
  await expect(signHostedHttp('bingx', { key: '', secret: '' }, true, host, place())).rejects.toThrow('Signing identity changed');
});

it.each([place, cancel, amend])('hosted capture and the legacy TS interceptor share one wire (%#)', async (request) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  const legacy = await signHttpCapture(createAPI({ key: 'owned-public', secret: 'fixture-material', tealstreetTunnel: false } as ExchangeOptions,
    { name: 'bingx', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), request());
  const { host } = owned(hmac);
  const hosted = await signHostedHttp('bingx', { key: '', secret: '' }, true, host, request());
  expect(hosted.config?.headers['X-BX-APIKEY']).toBe(legacy.config?.headers['X-BX-APIKEY']);
  expect(hosted.config?.headers['Content-Type']).toBe(legacy.config?.headers['Content-Type']);
  expect(hosted.config?.params).toEqual(legacy.config?.params);
  expect(hosted.config?.data).toBe(legacy.config?.data);
  expect(hosted.config?.url).toBe(legacy.config?.url);
  const signed = legacy.config?.params ?? JSON.parse(String(legacy.config!.data));
  expect(signed.signature).toMatch(/^[0-9a-f]{64}$/);
});

it('passes the hex login format to the owned capture', async () => {
  const hostedSigning = vi.fn(async () => ({ signature: 'owned-hex' }));
  await expect(signHostedWsAuth({ hostedSigning } as unknown as EngineHost, { id: 'ws', type: 'ws-auth', exchangeName: 'bingx',
    wsAuth: { toSign: placePreimage, outputFormat: 'hex' } })).resolves.toEqual({ id: 'ws', wsAuthSignature: 'owned-hex' });
  expect(hostedSigning).toHaveBeenCalledWith({ kind: 'wsAuth', toSign: placePreimage, outputFormat: 'hex' });
});
