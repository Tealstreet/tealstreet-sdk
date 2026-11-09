import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSignerRequest } from '../../types';

import { createHash, createHmac } from 'node:crypto';

import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import {
  deriveBridgeConstruction,
  deriveRemoteSigningPolicy,
  deriveSignerSetupPolicy,
} from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { signKrakenHttp } from '../../exchanges/rust/krakenHostedSigning';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';

const identity = { credential: 'ref:public', signing: 'ref:signing', publicCredential: 'fixture-key' };
const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const digest = (text: string) => createHash('sha256').update(text).digest();
const signature = (text: string) => createHmac('sha512', 'secret').update(digest(text)).digest('base64');
const work = (payload: string): SignRequest => ({
  receipt: { nonce: '1700000000000000', stampMs: 1_700_000_000_000 },
  context: { venue: 'advisory', method: 'POST', host: '', path: '' },
  operations: [
    { kind: 'present', key: identity.credential },
    { kind: 'sha256', payload: [{ kind: 'bytes', bytes: bytes(payload) }] },
    {
      kind: 'sign',
      algorithm: 'hmacSha512',
      key: identity.signing,
      payload: [{ kind: 'result', index: 1, encoding: 'raw' }],
    },
  ],
});
const signal = () => ({ signal: new AbortController().signal });

it('declares Futures remote custody and base64 setup without enabling Spot or PayloadSigner', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy('kraken')).toMatchObject({ enabled: true, payloadSupported: false });
  expect(remoteSignerAlgorithms('kraken')).toEqual(['hmacSha512']);
  expect(deriveSignerSetupPolicy('kraken').primary).toMatchObject({ secretEncoding: 'base64' });
  expect(deriveBridgeConstruction('kraken', { key: '', secret: '' })).toMatchObject({ dispatchSigning: true });
  expect(() => assertBetaAccount('kraken', { key: 'fixture-key', remoteSigner })).not.toThrow();
  expect(deriveRemoteSigningPolicy('krakenspot')).toBeUndefined();
  expect(() => assertBetaAccount('krakenspot', { key: 'fixture-key', remoteSigner }))
    .toThrow(/isn't available on the Hosted Server/);
  expect(
    () => new DelegatedSignerSetup('kraken', { key: 'fixture-key', secret: '', payloadSigner: vi.fn() }),
  ).toThrow();
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it.each([
  [
    'note=%C3%A9+%2B+%2F~&symbol=PF_XBTUSD1700000000000000/api/v3/sendorder',
    '4TMpQ1hDUBIUn95fYJYlhprS9SpGaPNvX5zdebA2b3LemKgBXCiar+BQwvj/3v0f6go1Qbmylp0YaLR3FUrRiA==',
  ],
  [
    'count=501700000000000001/api/history/v3/account-log',
    'NxNOdcerU3JDkpk81eVA1BWaCNmsoqCCCfL7TbOOoITZdM7elEFmHGTUu3NRcnqfgWmdDXDgq307kNY+0ZlKOg==',
  ],
  ['challenge-æ-123', 'RrIo22Xn634Pt3sq0W+1h/7OUdrAgbSG4qt+qaycGFEledqV4+k70KrF0smVdYMou8BVj/wi056PrIXGJfjRtQ=='],
])('remote graph matches independent local SHA256/HMAC-SHA512 vector %s', async (payload, golden) => {
  const sign = vi.fn(async (request: RemoteSignerRequest) => {
    expect(request.wsAuth?.outputFormat).toBe('kraken-sha512');
    expect(JSON.parse(request.wsAuth!.toSign)).toEqual({ slot: 'primary', payload });
    return {
      id: request.id,
      wsAuthSignature: JSON.stringify({ credential: 'fixture-key', signature: signature(payload) }),
    };
  });
  const provider = createRemoteSignerProvider({
    exchangeName: 'kraken',
    identities: { primary: identity },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} },
  });
  const result = await provider(work(payload), signal());
  expect(result.values).toEqual([
    bytes('fixture-key'),
    Array.from(digest(payload)),
    Array.from(Buffer.from(golden, 'base64')),
  ]);
  expect(result.receipt).toEqual(work(payload).receipt);
  expect(sign).toHaveBeenCalledOnce();
  expect(JSON.stringify(sign.mock.calls)).not.toMatch(/ref:|secret/);
});

it('refuses foreign refs, partial graphs, wrong-account answers and callback material without fallback', async () => {
  const sign = vi.fn(async (request: RemoteSignerRequest) => ({
    id: request.id,
    wsAuthSignature: JSON.stringify({ credential: 'other-account', signature: signature('challenge') }),
  }));
  const provider = createRemoteSignerProvider({
    exchangeName: 'kraken',
    identities: { primary: identity },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} },
  });
  for (const mutate of [
    (request: SignRequest) => {
      request.operations.pop();
    },
    (request: SignRequest) => {
      request.operations[0] = { kind: 'present', key: 'foreign' };
    },
    (request: SignRequest) => {
      request.operations[2] = {
        kind: 'sign',
        algorithm: 'hmacSha512',
        key: identity.signing,
        payload: [{ kind: 'bytes', bytes: [1] }],
      };
    },
  ]) {
    const request = work('challenge');
    mutate(request);
    await expect(provider(request, signal())).rejects.toThrow('Remote signing request refused');
  }
  expect(sign).not.toHaveBeenCalled();
  await expect(provider(work('challenge'), signal())).rejects.toThrow('Remote signing request refused');
  sign.mockImplementationOnce(async () => {
    throw new Error('fixture-private-material');
  });
  await expect(provider(work('challenge'), signal())).rejects.toThrow(/^Remote signing request refused$/);
});

it('selects secondary device custody and never forwards opaque refs', async () => {
  const secondary = {
    credential: 'ref:read-public',
    signing: 'ref:read-signing',
    publicCredential: 'fixture-read-key',
  };
  const sign = vi.fn(async (request: RemoteSignerRequest) => ({
    id: request.id,
    wsAuthSignature: JSON.stringify({ credential: secondary.publicCredential, signature: signature('read') }),
  }));
  const provider = createRemoteSignerProvider({
    exchangeName: 'kraken',
    identities: { primary: identity, secondary },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} },
  });
  const request = work('read');
  request.operations[0] = { kind: 'present', key: secondary.credential };
  request.operations[2] = {
    kind: 'sign',
    algorithm: 'hmacSha512',
    key: secondary.signing,
    payload: [{ kind: 'result', index: 1, encoding: 'raw' }],
  };
  const result = await provider(request, signal());
  expect(result.values[0]).toEqual(bytes(secondary.publicCredential));
  expect(JSON.parse(sign.mock.calls[0][0].wsAuth!.toSign)).toEqual({ slot: 'secondary', payload: 'read' });
});

it('cancels a pending remote response and refuses malformed signatures atomically', async () => {
  let finish!: (response: { id: string; wsAuthSignature: string }) => void;
  const sign = vi.fn(
    (_: RemoteSignerRequest) =>
      new Promise<{ id: string; wsAuthSignature: string }>((resolve) => {
        finish = resolve;
      }),
  );
  const provider = createRemoteSignerProvider({
    exchangeName: 'kraken',
    identities: { primary: identity },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} },
  });
  const controller = new AbortController();
  const pending = provider(work('challenge'), { signal: controller.signal });
  const rejected = expect(pending).rejects.toThrow('Remote signing canceled');
  controller.abort();
  finish({ id: sign.mock.calls[0][0].id, wsAuthSignature: 'late fixture-private-material' });
  await rejected;
  sign.mockImplementation(async (request) => ({
    id: request.id,
    wsAuthSignature: JSON.stringify({ credential: 'fixture-key', signature: 'AAAA' }),
  }));
  await expect(provider(work('challenge'), signal())).rejects.toThrow(/^Remote signing request refused$/);
});

it('compatibility HTTP signing preserves encoded form bytes and a frozen u64 nonce', async () => {
  const captures: string[] = [];
  const host = {
    hostedSigningClock: () => ({ stampMs: 1_700_000_000_000, revision: '1' }),
    hostedSigning: async (input: { payload: string }) => {
      const { payload } = JSON.parse(input.payload);
      captures.push(payload);
      return { credential: 'fixture-key', signature: signature(payload) };
    },
  };
  const request: RemoteSignerRequest = {
    id: 'http',
    type: 'http',
    exchangeName: 'kraken',
    config: {
      method: 'POST',
      baseURL: 'https://futures.kraken.com',
      url: '/derivatives/api/v3/sendorder',
      data: 'symbol=PF_XBTUSD&note=%C3%A9+%2B+%2F~',
      headers: { Nonce: '18446744073709551615', Authent: 'stale', APIKey: 'stale', 'X-Fixture': 'keep' },
    },
  };
  const before = structuredClone(request);
  const result = await signKrakenHttp(host as any, { key: '', secret: '' }, true, request);
  const preimage = `${request.config!.data}18446744073709551615/api/v3/sendorder`;
  expect(result.error).toBeUndefined();
  expect(captures).toEqual([preimage]);
  expect(result.config?.data).toBe(request.config!.data);
  expect(result.config?.headers).toMatchObject({
    Nonce: '18446744073709551615',
    APIKey: 'fixture-key',
    Authent: signature(preimage),
    'X-Fixture': 'keep',
  });
  expect(request).toEqual(before);
});
