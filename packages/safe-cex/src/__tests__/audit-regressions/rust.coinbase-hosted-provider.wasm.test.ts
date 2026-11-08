import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

const now = 1700000000000;
const key = ['organizations', 'fixture', 'apiKeys', 'fixture'].join('/');
const seed = Buffer.from('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60', 'hex');
const edKey = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
  format: 'der',
  type: 'pkcs8',
});
const live: RustEngineExchange[] = [];
afterEach(() => {
  live.splice(0).forEach((e) => e.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});
const fixture = (name: ExchangeName, options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => {
    throw new Error('No network in signing fixture');
  });
  setRustEngineTransport(() => ({
    now: () => now,
    prepareHttp: prepareRecordedHttp,
    request: io,
    connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const exchange = new RustEngineExchange(
    name,
    { key, secret: seed.toString('base64'), coinbaseJwtAlgorithm: 'edDsa', ...options } as ExchangeOptions,
    new DefaultStore(),
  );
  live.push(exchange);
  return { exchange, io };
};
const input = (uri?: string) =>
  [
    { alg: 'EdDSA', typ: 'JWT', kid: key, nonce: 'fixture-nonce' },
    { iss: 'cdp', aud: ['cdp_service'], nbf: now / 1000, exp: now / 1000 + 120, sub: key, ...(uri && { uri }) },
  ]
    .map((item) => Buffer.from(JSON.stringify(item)).toString('base64url'))
    .join('.');

it.each(['coinbase', 'coinbasespot'] as ExchangeName[])(
  'direct WASM %s keeps keyless and explicit public construction independent of signer credentials',
  async (name) => {
    const { WasmEngine } = await loadNodeEngine();
    const stale = { key: 'stale', secret: 'malformed' };
    const cases = [
      { options: {} },
      { options: { secret: 'malformed', readCredentials: stale } },
      { options: { key: '', secret: 'malformed', readCredentials: stale } },
      { options: { publicDataOnly: true, ...stale, readCredentials: stale } },
      { options: { publicDataOnly: true }, capsule: { primary: stale, secondary: stale } },
      { options: {}, capsule: { secondary: stale } },
      { options: {}, capsule: { primary: { key: '', secret: 'malformed' }, secondary: stale } },
    ];
    for (const { options, capsule } of cases) {
      const engine = new WasmEngine(name, JSON.stringify(options), capsule && JSON.stringify(capsule));
      try {
        expect(() => engine.coinbaseSigningMetadata!()).toThrow('Signing provider is unavailable');
        type Packet = { url?: string; headers?: [string, string][] };
        const actions = JSON.parse(engine.start(now)) as (Packet & { op: string; packet?: Packet })[];
        const requests = actions.flatMap((action) =>
          action.op === 'http' ? [action] : action.op === 'managedHttp' && action.packet ? [action.packet] : [],
        );
        expect(requests.length).toBeGreaterThan(0);
        for (const request of requests) {
          expect(request.url).toContain(name === 'coinbasespot' ? '/market/products' : 'drb.coinbase.com');
          expect(request.headers?.some(([header]) => header.toLowerCase() === 'authorization')).toBe(false);
        }
        expect(actions.some((action) => action.op === 'sign')).toBe(false);
      } finally {
        engine.free?.();
      }
    }
  },
);

it.each(['coinbase', 'coinbasespot'] as ExchangeName[])(
  'actual WASM %s signs exact native REST and WS JWS through device custody',
  async (name) => {
    const browser = fixture(name);
    const callback = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
    const remoteSigner = { sign: callback, isAvailable: () => true, onAvailabilityChange: () => () => {} };
    const provider = createRemoteSignerProvider({
      exchangeName: name,
      identities: { primary: { credential: 'public', signing: 'private', publicCredential: key } },
      remoteSigner,
    });
    for (const uri of [
      undefined,
      'GET api.coinbase.com/api/v3/brokerage/accounts',
      'POST api.coinbase.com/api/v3/brokerage/orders',
      'POST api.coinbase.com/api/v3/brokerage/orders/edit',
      'POST api.coinbase.com/api/v3/brokerage/orders/batch_cancel',
      'POST drb.coinbase.com/api/v2/public/auth',
    ]) {
      const payload = input(uri);
      const work: SignRequest = {
        receipt: { stampMs: now, expiryMs: now + 120000 },
        context: { venue: name, method: 'WS', host: '', path: '' },
        operations: [
          { kind: 'present', key: 'public' },
          {
            kind: 'sign',
            key: 'private',
            algorithm: 'ed25519',
            payload: [{ kind: 'bytes', bytes: Array.from(Buffer.from(payload)) }],
          },
        ],
      };
      const result = await provider(work, { signal: new AbortController().signal });
      expect(result.values).toEqual([
        Array.from(Buffer.from(key)),
        Array.from(sign(null, Buffer.from(payload), edKey)),
      ]);
    }
    const server = fixture(name, { secret: '', remoteSigner });
    const response = await server.exchange.signHttpRequest({
      id: 'rest',
      type: 'http',
      exchangeName: name,
      config: {
        method: 'GET',
        baseURL: 'https://api.coinbase.com',
        url: '/api/v3/brokerage/accounts?cursor=abc',
        headers: { Authorization: 'discard-old' },
      },
    });
    expect(response.error).toBeUndefined();
    const jwt = String(response.config!.headers.Authorization).slice(7);
    const [header, claims, signature] = jwt.split('.');
    expect(JSON.parse(Buffer.from(claims, 'base64url').toString())).toMatchObject({
      uri: 'GET api.coinbase.com/api/v3/brokerage/accounts',
    });
    expect(
      verify(null, Buffer.from(`${header}.${claims}`), createPublicKey(edKey), Buffer.from(signature, 'base64url')),
    ).toBe(true);
    expect(server.exchange.options).not.toHaveProperty('secret');
    expect(server.exchange.options).not.toHaveProperty('remoteSigner');
    expect(browser.io).not.toHaveBeenCalled();
    expect(server.io).not.toHaveBeenCalled();
  },
);

it('actual WASM rejects foreign key, malformed claims and expired JWT without a signature', async () => {
  const browser = fixture('coinbasespot');
  const payload = input();
  for (const toSign of [
    payload.replace(
      payload.split('.')[0],
      Buffer.from(
        JSON.stringify({ alg: 'EdDSA', typ: 'JWT', kid: 'organizations/foreign/apiKeys/foreign', nonce: 'fixture' }),
      ).toString('base64url'),
    ),
    `${payload}.extra`,
    input().replace(
      payload.split('.')[1],
      Buffer.from(JSON.stringify({ iss: 'cdp', nbf: 1, exp: 121, sub: key })).toString('base64url'),
    ),
  ]) {
    const response = await browser.exchange.signWsAuth({
      id: 'invalid',
      type: 'ws-auth',
      exchangeName: 'coinbasespot',
      wsAuth: { toSign, outputFormat: 'coinbase-jws' },
    });
    expect(response.wsAuthSignature).toBeUndefined();
    expect(response.error).toBeTruthy();
  }
  expect(browser.io).not.toHaveBeenCalled();
});

it('actual WASM ES256 uses the same canonical JWS as the native signer and verifies independently', async () => {
  const { createECDH } = await import('node:crypto');
  const curve = createECDH('prime256v1');
  curve.setPrivateKey(Buffer.alloc(32, 7));
  const publicBytes = curve.getPublicKey();
  const p256Key = createPrivateKey({
    key: {
      kty: 'EC',
      crv: 'P-256',
      d: Buffer.alloc(32, 7).toString('base64url'),
      x: publicBytes.subarray(1, 33).toString('base64url'),
      y: publicBytes.subarray(33).toString('base64url'),
    },
    format: 'jwk',
  });
  const pem = p256Key.export({ type: 'pkcs8', format: 'pem' }).toString();
  const browser = fixture('coinbase', { secret: pem, coinbaseJwtAlgorithm: 'es256' });
  const payload = [
    { alg: 'ES256', typ: 'JWT', kid: key, nonce: 'fixture-nonce' },
    {
      iss: 'coinbase-cloud',
      nbf: now / 1000,
      exp: now / 1000 + 120,
      sub: key,
      uri: 'POST api.coinbase.com/api/v3/brokerage/orders',
    },
  ]
    .map((item) => Buffer.from(JSON.stringify(item)).toString('base64url'))
    .join('.');
  const work: RemoteSignerRequest = {
    id: 'p256',
    type: 'ws-auth',
    exchangeName: 'coinbase',
    wsAuth: { toSign: payload, outputFormat: 'coinbase-jws' },
  };
  const first = await browser.exchange.signWsAuth(work);
  expect(first.error).toBeUndefined();
  expect(await browser.exchange.signWsAuth(work)).toEqual(first);
  expect(
    verify(
      'sha256',
      Buffer.from(payload),
      { key: createPublicKey(p256Key), dsaEncoding: 'ieee-p1363' },
      Buffer.from(first.wsAuthSignature!, 'base64url'),
    ),
  ).toBe(true);
  const remoteSigner = {
    sign: (request: RemoteSignerRequest) => browser.exchange.signWsAuth(request),
    isAvailable: () => true,
    onAvailabilityChange: () => () => {},
  };
  const provider = createRemoteSignerProvider({
    exchangeName: 'coinbase',
    identities: { primary: { credential: 'public', signing: 'private', publicCredential: key } },
    remoteSigner,
  });
  const result = await provider(
    {
      receipt: { stampMs: now, expiryMs: now + 120000 },
      context: { venue: 'coinbase', method: 'POST', host: '', path: '' },
      operations: [
        { kind: 'present', key: 'public' },
        {
          kind: 'sign',
          key: 'private',
          algorithm: 'es256',
          payload: [{ kind: 'bytes', bytes: Array.from(Buffer.from(payload)) }],
        },
      ],
    },
    { signal: new AbortController().signal },
  );
  expect(Buffer.from(result.values[1]).toString('base64url')).toBe(first.wsAuthSignature);
});

it('actual WASM PayloadSigner has REST-only custody and matches local JWS bytes', async () => {
  const callback = vi.fn((request: { payload: string }) => ({
    signature: sign(null, Buffer.from(request.payload), edKey).toString('base64url'),
  }));
  const exchange = fixture('coinbasespot', { secret: '', payloadSigner: callback });
  const response = await exchange.exchange.signHttpRequest({
    id: 'payload',
    type: 'http',
    exchangeName: 'coinbasespot',
    config: {
      baseURL: 'https://api.coinbase.com',
      method: 'POST',
      url: '/api/v3/brokerage/orders',
      data: '{"size":"1"}',
    },
  });
  expect(response.error).toBeUndefined();
  const jwt = String(response.config!.headers.Authorization).slice(7);
  const [head, claims, signature] = jwt.split('.');
  expect(
    verify(null, Buffer.from(`${head}.${claims}`), createPublicKey(edKey), Buffer.from(signature, 'base64url')),
  ).toBe(true);
  expect(callback).toHaveBeenCalledExactlyOnceWith({ exchangeName: 'coinbasespot', key, payload: `${head}.${claims}`, scheme: 'coinbase-jws', encoding: 'base64url', algorithm: 'ed25519' });
  expect(exchange.exchange.options).not.toHaveProperty('payloadSigner');
  const ws = await exchange.exchange.signWsAuth({
    id: 'ws',
    type: 'ws-auth',
    exchangeName: 'coinbasespot',
    wsAuth: { toSign: input(), outputFormat: 'coinbase-jws' },
  });
  expect(ws.wsAuthSignature).toBeUndefined();
  expect(callback).toHaveBeenCalledOnce();
  expect(exchange.io).not.toHaveBeenCalled();
});
