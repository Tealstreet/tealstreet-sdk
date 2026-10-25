import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSignerRequest } from '../../types';

import { createHash, createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { signHostedHttp } from '../../exchanges/rust/legacySigning';

const stamp = 1_700_000_000_000;
const key = 'fixture-kraken-public';
const secret = Buffer.from('c2VjcmV0', 'base64');
const futuresForm = 'orderType=lmt&size=1&symbol=PF_XBTUSD';
const spotForm = 'pair=XBTUSD&type=buy&ordertype=limit&volume=1&price=100';
const vectors = [
  {
    nonce: '9007199254740993',
    futures: 'Wa+5d2RGeXTakUvpqsmmtgQL3RgIXHBj4v38Y8KPJjt3aWez/zGUqFBQZTal27SRaBESVyzKG8r4dIGUWnY5jA==',
    spot: 'uJpIMuN/xno7B73sMKerNiX1bLsh3q674EtYc5VOb2qSSq04OWImK3DKDT0dzSjMe91eyZrrLO1Q5SHPSbqHeA==',
  },
  {
    nonce: '9007199254740995',
    futures: 'LAWB45elTMC7fvsSK58dMMGWGPu93He7vn4gOfHUM3e8q4B3IVHZhD4xZLiXdP4ozF+AeS+vnScXJodEkFxN0w==',
    spot: 'IUShC39q8hm8MiXx9nHggM/XMWo0iqI2omHmBNmvpYllsEPceLJ1uy7OApT00FW4s3fyesQAZxDvDveD2eJSFA==',
  },
  {
    nonce: '18446744073709551615',
    futures: 'xjJIL9ClDcLoH6ytlKcxQunYAQxJwza9xFp/4bSSnCdJznsXqo4TVp5GqPscQ90LAKBEWVM4pfV8aLK75NgFzw==',
    spot: 'Kw7VRGQ/R+Mxw7sJl9jLLGAWDMZgnAsHsglCRFhL3QJhXYAttUbxgClOdBONSH5zyc68EQRMGq6scxrKS7g9Ow==',
  },
] as const;

type Capture = {
  kind: string;
  nonce?: string;
  form?: string;
  path?: string;
  payload?: string;
};
const independentMac = (spot: boolean, nonce: string, form: string, path: string) => {
  const body = `${form}&nonce=${nonce}`;
  const digest = createHash('sha256').update(spot ? nonce + body : form + nonce + path).digest();
  return createHmac('sha512', secret).update(spot ? Buffer.concat([Buffer.from(path), digest]) : digest).digest('base64');
};

it('pins six literal MACs independently with node:crypto; MAX is codec/MAC evidence only', () => {
  for (const vector of vectors) {
    expect(independentMac(false, vector.nonce, futuresForm, '/api/v3/sendorder')).toBe(vector.futures);
    expect(independentMac(true, vector.nonce, spotForm, '/0/private/AddOrder')).toBe(vector.spot);
  }
});

// This fixture checks the SDK codec boundary, not real WASM custody. The
// separately hashed actual-engine/recipe/peer proof is a later mandatory gate.
const peer = (spot: boolean) => {
  const hostedSigning = vi.fn(async (input: Capture) => {
    if (input.kind === 'credential') return { credential: key };
    if (input.kind !== 'krakenHttp' || !input.nonce || input.form === undefined || !input.path
      || !/^[1-9][0-9]*$/.test(input.nonce) || BigInt(input.nonce) > 18_446_744_073_709_551_615n) {
      throw new Error('Invalid exact-nonce capture');
    }
    return { credential: key, signature: independentMac(spot, input.nonce, input.form, input.path) };
  });
  const host = {
    hostedSigning,
    hostedSigningClock: vi.fn(() => ({ stampMs: stamp, revision: '7' })),
  } as unknown as EngineHost;
  return { host, hostedSigning };
};

it.each(vectors)('Futures forwards the exact reserved decimal nonce $nonce without allocating again', async (vector) => {
  const { host, hostedSigning } = peer(false);
  const request: RemoteSignerRequest = {
    id: 'exact-futures', type: 'http', exchangeName: 'kraken',
    config: {
      method: 'POST', baseURL: 'https://futures.kraken.com', url: '/derivatives/api/v3/sendorder',
      data: futuresForm, headers: { Nonce: vector.nonce },
    },
  };
  const before = structuredClone(request);
  const output = await signHostedHttp('kraken', { key: '', secret: '' }, true, host, request);
  expect(output.error, 'the exact-nonce adapter must be a working route').toBeUndefined();
  expect(output.config?.url).toBe(request.config!.url);
  expect(output.config?.data).toBe(futuresForm);
  expect(output.config?.headers).toMatchObject({ APIKey: key, Authent: vector.futures, Nonce: vector.nonce });
  expect(hostedSigning).toHaveBeenCalledWith(expect.objectContaining({
    kind: 'krakenHttp', nonce: vector.nonce, form: futuresForm, path: '/api/v3/sendorder',
  }));
  expect(request).toEqual(before);
});

it.each(vectors)('Spot preserves nonce-last body $nonce and signs that exact preimage', async (vector) => {
  const { host, hostedSigning } = peer(true);
  const body = `${spotForm}&nonce=${vector.nonce}`;
  const request: RemoteSignerRequest = {
    id: 'exact-spot', type: 'http', exchangeName: 'krakenspot',
    config: { method: 'POST', baseURL: 'https://api.kraken.com', url: '/0/private/AddOrder', data: body },
  };
  const before = structuredClone(request);
  const output = await signHostedHttp('krakenspot', { key: '', secret: '' }, true, host, request);
  expect(output.error, 'the exact-nonce adapter must be a working route').toBeUndefined();
  expect(output.config?.data).toBe(body);
  expect(output.config?.headers).toMatchObject({ 'API-Key': key, 'API-Sign': vector.spot });
  expect(hostedSigning).toHaveBeenCalledWith(expect.objectContaining({
    kind: 'krakenHttp', nonce: vector.nonce, form: spotForm, path: '/0/private/AddOrder',
  }));
  expect(request).toEqual(before);
});

it('keeps the existing HmacSha256 hosted Bybit route working as a control', async () => {
  const hostedSigning = vi.fn(async (input: Capture) => input.kind === 'credential'
    ? { credential: key }
    : { credential: key, signature: createHmac('sha256', 'control-secret').update(input.payload!).digest('hex') });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '7' }) } as unknown as EngineHost;
  const output = await signHostedHttp('bybit', { key: '', secret: '' }, true, host, {
    id: 'working-control', type: 'http', exchangeName: 'bybit',
    config: { method: 'GET', baseURL: 'https://api.bybit.com', url: '/v5/account/wallet-balance', params: { accountType: 'UNIFIED' } },
  });
  expect(output.error).toBeUndefined();
  const payload = `${stamp}${key}5000accountType=UNIFIED`;
  expect(output.config?.headers['X-BAPI-SIGN']).toBe(createHmac('sha256', 'control-secret').update(payload).digest('hex'));
  expect(hostedSigning).toHaveBeenCalledWith({ kind: 'http', payload, stampMs: stamp, revision: '7' });
});
