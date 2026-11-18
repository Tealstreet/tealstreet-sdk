import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';

import { afterEach, expect, it, vi } from 'vitest';

import vectors from '../../../../../sdk-rs/venues/cryptocom/tests/fixtures/hosted-signing.json';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { signHostedHttp, signHostedWsAuth } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';
import { createAPI } from '../reference/cryptocom/cryptocom.api';

const stamp = 1_610_000_000_000;
const mac = (payload: string) => createHmac('sha256', 'replay-secret').update(payload).digest('hex');
const owned = (now = stamp) => {
  const hostedSigning = vi.fn(async (input: { kind: string; payload?: string; toSign?: string }) =>
    input.kind === 'credential'
      ? { credential: 'replay-key' }
      : { credential: 'replay-key', signature: mac(input.payload ?? input.toSign!) },
  );
  return {
    hostedSigning,
    host: { hostedSigning, hostedSigningClock: () => ({ stampMs: now, revision: '1' }) } as unknown as EngineHost,
  };
};
afterEach(() => vi.restoreAllMocks());

it.each(vectors)('Crypto.com $method: HTTP metadata preserves the local JSON signature bytes', async (v) => {
  const host = {
    hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }),
    hostedSigning: async (input: { kind: string; payload?: string }) =>
      input.kind === 'credential'
        ? { credential: 'replay-key' }
        : { credential: 'replay-key', signature: mac(input.payload!), headers: [['X-Trace', 'keep']] },
  } as unknown as EngineHost;
  const result = await signHostedHttp('cryptocom', {} as ExchangeOptions, true, host, {
    id: 'metadata',
    type: 'http',
    exchangeName: 'cryptocom',
    config: {
      method: 'POST',
      baseURL: 'https://api.crypto.com/exchange/v1',
      url: `/${v.method}`,
      data: v.params,
      headers: {},
    },
  });
  expect(result.error).toBeUndefined();
  expect(result.config?.headers['X-Trace']).toBe('keep');
  expect(JSON.parse(result.config!.data as string).sig).toBe(v.signature);
  expect(result.config?.headers).not.toHaveProperty('sig');
});

it('re-signs a serialized envelope using only its original params and a fresh protocol timestamp', async () => {
  const v = vectors.find((v) => v.method === 'private/create-order')!;
  const params = v.params;
  const { host, hostedSigning } = owned(stamp + 500);
  const result = await signHostedHttp('cryptocom', {} as ExchangeOptions, true, host, {
    id: 'replay',
    type: 'http',
    exchangeName: 'cryptocom',
    config: {
      method: 'POST',
      baseURL: 'https://api.crypto.com/exchange/v1',
      url: `/${v.method}`,
      headers: {},
      data: JSON.stringify({
        id: stamp,
        method: v.method,
        api_key: 'replay-key',
        params,
        nonce: stamp,
        sig: v.signature,
      }),
    },
  });
  expect(result.error).toBeUndefined();
  const body = JSON.parse(result.config!.data as string);
  expect(body).toMatchObject({ id: stamp + 500, nonce: stamp + 500, params });
  const payload = `${v.method}${stamp + 500}${v.payload.slice(v.method.length + 13, -13)}${stamp + 500}`;
  expect(body.sig).toBe(mac(payload));
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', payload, stampMs: stamp + 500, revision: '1' });
});

it('leaves public reads unsigned and preserves their query parameters', async () => {
  const { host, hostedSigning } = owned();
  const result = await signHostedHttp('cryptocom', {} as ExchangeOptions, true, host, {
    id: 'public',
    type: 'http',
    exchangeName: 'cryptocom',
    config: {
      method: 'POST',
      baseURL: 'https://api.crypto.com/exchange/v1',
      url: '/public/get-book',
      data: { instrument_name: 'BTCUSD-PERP', depth: 50 },
      headers: {},
    },
  });
  expect(result.error).toBeUndefined();
  expect(result.config).toMatchObject({ params: { instrument_name: 'BTCUSD-PERP', depth: 50 } });
  expect(result.config?.data).toBeUndefined();
  expect(hostedSigning).not.toHaveBeenCalled();
});

it('canonicalizes nested arrays, objects and null without JSON separators', async () => {
  const { host, hostedSigning } = owned();
  const params = {
    exec_inst: ['REDUCE_ONLY', 'POST_ONLY'],
    order_list: [
      { side: 'BUY', quantity: '1' },
      { side: 'SELL', quantity: null },
    ],
  };
  const payload = `private/create-order-list${stamp}replay-keyexec_instREDUCE_ONLYPOST_ONLYorder_listquantity1sideBUYquantitynullsideSELL${stamp}`;
  const result = await signHostedHttp('cryptocom', {} as ExchangeOptions, true, host, {
    id: 'nested',
    type: 'http',
    exchangeName: 'cryptocom',
    config: {
      method: 'POST',
      baseURL: 'https://api.crypto.com/exchange/v1',
      url: '/private/create-order-list',
      data: params,
      headers: {},
    },
  });
  expect(result.error).toBeUndefined();
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', payload, stampMs: stamp, revision: '1' });
  expect(JSON.parse(result.config!.data as string).sig).toBe(mac(payload));
});

it.each(vectors)(
  'Crypto.com $method: hosted canonical payload and complete envelope match local signing',
  async (v) => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
    const request: RemoteSignerRequest = {
      id: v.method,
      type: 'http',
      exchangeName: 'cryptocom',
      config: {
        method: 'POST',
        url: `/${v.method}`,
        baseURL: 'https://api.crypto.com/exchange/v1',
        headers: { 'X-Transport': 'keep' },
        data: v.params,
      },
    };
    const before = structuredClone(request);
    const { host, hostedSigning } = owned();
    const remote = await signHostedHttp('cryptocom', {} as ExchangeOptions, true, host, request);
    expect(remote.error).toBeUndefined();
    const api = createAPI(
      { key: 'replay-key', secret: 'replay-secret' } as ExchangeOptions,
      { name: 'cryptocom', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit() {} } } as never,
    );
    api.interceptors.response.clear();
    const local = await signHttpCapture(api, request);
    expect(local.error).toBeUndefined();
    expect(remote.config?.data).toBe(local.config?.data);
    expect(JSON.parse(remote.config!.data as string).sig).toBe(v.signature);
    expect(remote.config?.headers).toEqual(local.config?.headers);
    expect(hostedSigning.mock.calls).toEqual([
      [{ kind: 'credential' }],
      [{ kind: 'http', payload: v.payload, stampMs: stamp, revision: '1' }],
    ]);
    expect(request).toEqual(before);
  },
);

it('admits Crypto.com RemoteSigner and canonical PayloadSigner while retaining undeclared refusal', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy('cryptocom')).toMatchObject({
    enabled: true,
    payloadSupported: true,
    payloadRestOnly: true,
  });
  expect(deriveBridgeConstruction('cryptocom', {} as ExchangeOptions).dispatchSigning).toBe(true);
  expect(() => assertBetaAccount('cryptocom', { key: 'replay-key', remoteSigner })).not.toThrow();
  expect(() => assertBetaAccount('cryptocom', { key: 'replay-key', payloadSigner: vi.fn() })).not.toThrow();
  expect(deriveRemoteSigningPolicy('krakenspot')).toBeUndefined();
  expect(() => assertBetaAccount('krakenspot', { key: 'replay-key', remoteSigner })).toThrow(
    expect.objectContaining({ kind: 'unsupported', outcome: 'notSent',
      message: expect.stringContaining("isn't available on the Hosted Server") }),
  );
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it('signs the private WS authentication preimage with the same HMAC as local custody', async () => {
  const { host, hostedSigning } = owned();
  const payload = `public/auth${stamp}replay-key${stamp}`;
  const result = await signHostedWsAuth(host, {
    id: 'auth',
    type: 'ws-auth',
    exchangeName: 'cryptocom',
    wsAuth: { toSign: payload, outputFormat: 'hex' },
  });
  expect(result).toEqual({
    id: 'auth',
    wsAuthSignature: createHmac('sha256', 'replay-secret').update('public/auth1610000000000replay-key1610000000000').digest('hex'),
  });
  expect(hostedSigning).toHaveBeenCalledWith({ kind: 'wsAuth', toSign: payload, outputFormat: 'hex' });
});

it.each([{ secondary_key: 'read-key' }, { secondary_secret: 'read-material' }, { localSigner: { secondary: {} } }])(
  'refuses secondary RemoteSigner custody before callbacks: %j',
  (secondary) => {
    const remoteSigner = { sign: vi.fn(), isAvailable: vi.fn(), onAvailabilityChange: vi.fn() };
    expect(
      () => new DelegatedSignerSetup('cryptocom', { key: 'replay-key', secret: '', remoteSigner, ...secondary }),
    ).toThrow(
      expect.objectContaining({
        kind: 'unsupported',
        outcome: 'notSent',
        message: expect.stringContaining('Crypto.com secondary'),
      }),
    );
    expect(remoteSigner.sign).not.toHaveBeenCalled();
    expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
  },
);
