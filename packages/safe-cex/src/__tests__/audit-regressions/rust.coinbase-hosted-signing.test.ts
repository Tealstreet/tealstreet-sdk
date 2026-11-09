import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createPrivateKey, sign } from 'node:crypto';

import { expect, it, vi } from 'vitest';

import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';

const key = ['organizations', 'fixture', 'apiKeys', 'fixture'].join('/');
// RFC 8032 test 1, published synthetic seed.
const seed = Buffer.from('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60', 'hex');
const privateKey = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
  format: 'der',
  type: 'pkcs8',
});
const input = (uri?: string) =>
  [
    { alg: 'EdDSA', typ: 'JWT', kid: key, nonce: 'fixture-nonce' },
    { iss: 'cdp', aud: ['cdp_service'], nbf: 1700000000, exp: 1700000120, sub: key, ...(uri && { uri }) },
  ]
    .map((value) => Buffer.from(JSON.stringify(value)).toString('base64url'))
    .join('.');
const request = (toSign: string): SignRequest => ({
  receipt: { stampMs: 1700000000000, expiryMs: 1700000120000 },
  context: { venue: 'coinbase', method: 'WS', host: '', path: '' },
  operations: [
    { kind: 'present', key: 'public' },
    {
      kind: 'sign',
      key: 'private',
      algorithm: 'ed25519',
      payload: [{ kind: 'bytes', bytes: Array.from(Buffer.from(toSign)) }],
    },
  ],
});
const remote = (callback: (value: RemoteSignerRequest) => unknown) => ({
  sign: vi.fn(callback),
  isAvailable: () => true,
  onAvailabilityChange: () => () => {},
});

it.each(['coinbase', 'coinbasespot'] as ExchangeName[])(
  '%s declares JWT capabilities and a precise secondary signing refusal',
  (name) => {
    expect(deriveRemoteSigningPolicy(name)).toMatchObject({
      enabled: true,
      payloadSupported: true,
      payloadRestOnly: true,
    });
    expect(remoteSignerAlgorithms(name)).toEqual(['es256', 'ed25519']);
    expect(
      () =>
        new DelegatedSignerSetup(name, {
          key,
          remoteSigner: remote(vi.fn()),
          secondary_key: 'read',
          secondary_secret: '',
        } as unknown as ExchangeOptions),
    ).toThrow(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }));
  },
);

it.each([undefined, 'GET api.coinbase.com/api/v3/brokerage/accounts', 'POST api.coinbase.com/api/v3/brokerage/orders'])(
  'remote JWS matches local Ed25519 bytes for %s',
  async (uri) => {
    const payload = input(uri);
    const signature = sign(null, Buffer.from(payload), privateKey);
    const delegate = remote((work) => ({
      id: work.id,
      wsAuthSignature: sign(null, Buffer.from(work.wsAuth!.toSign), privateKey).toString('base64url'),
    }));
    const provider = createRemoteSignerProvider({
      exchangeName: 'coinbase',
      identities: { primary: { credential: 'public', signing: 'private', publicCredential: key } },
      remoteSigner: delegate as never,
    });
    const result = await provider(request(payload), { signal: new AbortController().signal });
    expect(result.values).toEqual([Array.from(Buffer.from(key)), Array.from(signature)]);
  const fractional = request(payload);
  fractional.receipt.stampMs! += 0.5;
  expect((await provider(fractional, { signal: new AbortController().signal })).receipt).toEqual(fractional.receipt);
    expect(delegate.sign).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'ws-auth',
        exchangeName: 'coinbase',
        wsAuth: { toSign: payload, outputFormat: 'coinbase-jws' },
      }),
    );
  },
);

it('does not invoke a delegate for foreign key, algorithm, extra operations or malformed bytes', async () => {
  const delegate = remote(() => {
    throw new Error('must not be called');
  });
  const provider = createRemoteSignerProvider({
    exchangeName: 'coinbase',
    identities: { primary: { credential: 'public', signing: 'private', publicCredential: key } },
    remoteSigner: delegate as never,
  });
  const work = request(input());
  const invalid = [
    request(input().split('.').map((part, index) => index === 1 ? Buffer.from(JSON.stringify({ ...JSON.parse(Buffer.from(part, 'base64url').toString()), extra: true })).toString('base64url') : part).join('.')),
    { ...work, operations: [...work.operations, work.operations[0]] },
    { ...work, operations: [work.operations[0], { ...work.operations[1], algorithm: 'hmacSha256' }] },
    request(
      input().replace(
        input().split('.')[0],
        Buffer.from(JSON.stringify({ alg: 'EdDSA', kid: 'foreign' })).toString('base64url'),
      ),
    ),
  ];
  for (const value of invalid)
    await expect(provider(value as SignRequest, { signal: new AbortController().signal })).rejects.toThrow(
      'Remote signing request refused',
    );
  expect(delegate.sign).not.toHaveBeenCalled();
});

it('remote errors are redacted and never fall back to PayloadSigner', async () => {
  const fallback = vi.fn();
  const delegate = remote(() => {
    throw new Error('synthetic private failure');
  });
  const provider = createRemoteSignerProvider({
    exchangeName: 'coinbasespot',
    identities: { primary: { credential: 'public', signing: 'private', publicCredential: key } },
    remoteSigner: delegate as never,
    payloadSigner: fallback,
  });
  await expect(provider(request(input()), { signal: new AbortController().signal })).rejects.toThrow(
    'Remote signing request refused',
  );
  expect(fallback).not.toHaveBeenCalled();
});

it('PayloadSigner signs canonical REST JWS and refuses WS auth without invoking the callback', async () => {
  const callback = vi.fn((request: { payload: string }) => ({
    signature: sign(null, Buffer.from(request.payload), privateKey).toString('base64url'),
  }));
  const provider = createRemoteSignerProvider({
    exchangeName: 'coinbase',
    identities: { primary: { credential: 'public', signing: 'private', publicCredential: key } },
    payloadSigner: callback,
  });
  const payload = input('POST api.coinbase.com/api/v3/brokerage/orders');
  const result = await provider(request(payload), { signal: new AbortController().signal });
  expect(result.values[1]).toEqual(Array.from(sign(null, Buffer.from(payload), privateKey)));
  expect(callback).toHaveBeenCalledExactlyOnceWith({ exchangeName: 'coinbase', key, payload, scheme: 'coinbase-jws', encoding: 'base64url', algorithm: 'ed25519' });
  await expect(provider(request(input()), { signal: new AbortController().signal })).rejects.toThrow(
    'Remote signing request refused',
  );
  expect(callback).toHaveBeenCalledOnce();
});
