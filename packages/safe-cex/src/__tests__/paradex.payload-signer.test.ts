import type { ParadexSDKAccount } from './reference/paradex/paradex.sdk';
import type { ExchangeOptions, PayloadSignerRequest } from '../types';

import '../transport/registerTealstreetTransport';

import { Account, ec } from 'starknet';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { axiosHttpOverWebsocketAdapter } from '../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { createAPI } from './reference/paradex/paradex.api';
import { ParadexJWTManager, ParadexRequestSigner } from './reference/paradex/paradex.sdk';

const ADDRESS = '0x4f3c1d2e5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2';
const CONFIG = { paradexChainId: 'PRIVATE_SN_POTC_SEPOLIA' };
const ORDER = { market: 'BTC-USD-PERP', side: 'BUY' as const, type: 'LIMIT', size: '0.01', price: '65000' };

const remote = () => {
  const calls: PayloadSignerRequest[] = [];
  const options = {
    key: '0xpub',
    payloadSigner: (request: PayloadSignerRequest) => {
      calls.push(request);
      return { signature: 'signature-from-signer', headers: { 'X-Signer-Header': request.payload } };
    },
  } as unknown as ExchangeOptions;
  const sdkAccount = { account: { address: ADDRESS }, config: CONFIG, provider: {} } as ParadexSDKAccount;
  return { calls, options, sdkAccount };
};

describe('Paradex with payloadSigner', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('asks the signer for the same hash a local key signs, and places its answer verbatim', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_784_505_600_123);
    const { calls, options, sdkAccount } = remote();
    const signed = await new ParadexRequestSigner(sdkAccount, options).signOrder(ORDER);

    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({ exchangeName: 'paradex', key: '0xpub', scheme: 'stark', encoding: 'felt-pair' });
    expect(signed.signature).toBe('signature-from-signer');
    expect(signed.headers).toEqual({ 'X-Signer-Header': calls[0].payload });

    const privateKey = '0x' + Buffer.from(ec.starkCurve.utils.randomPrivateKey()).toString('hex');
    const local = {
      account: new Account({ nodeUrl: 'http://127.0.0.1:1' }, ADDRESS, privateKey),
      config: CONFIG,
      provider: {},
    } as ParadexSDKAccount;
    const localSigned = await new ParadexRequestSigner(local, {} as ExchangeOptions).signOrder(ORDER);
    const [r, s] = JSON.parse(localSigned.signature).map(BigInt);

    expect(
      ec.starkCurve.verify(new ec.starkCurve.Signature(r, s), calls[0].payload, ec.starkCurve.getPublicKey(privateKey)),
    ).toBe(true);
  });

  it('sends the signer answer and headers on the JWT auth request', async () => {
    const { calls, options, sdkAccount } = remote();
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ jwt_token: 'jwt' }), { status: 200 }));

    await expect(new ParadexJWTManager(sdkAccount, true, '0xsubkey', options).getToken()).resolves.toBe('jwt');

    const headers = fetchMock.mock.calls[0][1]!.headers as Record<string, string>;
    expect(headers['PARADEX-STARKNET-SIGNATURE']).toBe('signature-from-signer');
    expect(headers['X-Signer-Header']).toBe(calls[0].payload);
    expect(fetchMock.mock.calls[0][0]).toBe('https://api.testnet.paradex.trade/v1/auth/0xsubkey');
  });

  // A payloadSigner caller controls the request's transport, so private requests go direct.
  it('sends private requests direct, not through the Tealstreet tunnel', () => {
    const { options } = remote();
    const xhr = createAPI({ ...options, paradexAddress: ADDRESS, subkeyAddress: '0xpub' }, {} as any);

    expect(xhr.defaults.adapter).not.toBe(axiosHttpOverWebsocketAdapter);
    expect(createAPI({ key: '0xpriv', paradexAddress: ADDRESS } as ExchangeOptions, {} as any).defaults.adapter).toBe(
      axiosHttpOverWebsocketAdapter,
    );
  });
});
