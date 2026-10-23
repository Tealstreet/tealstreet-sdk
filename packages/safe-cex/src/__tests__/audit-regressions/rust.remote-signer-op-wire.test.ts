import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { PayloadSigner, RemoteSigner, RemoteSignerRequest, RemoteSignerResponse } from '../../types';

import { expect, it, vi } from 'vitest';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';

// P8 wire v2: an engine-bound recipe op crosses as `type: 'op', v: 2` only to a
// signer that advertises that exact recipe id; HMAC work is unchanged.
vi.mock('nanoid', () => ({ nanoid: () => 'fixed-op-id' }));
const primary = { credential: 'ref:primary-public', signing: 'ref:primary-signing', publicCredential: 'ed25519:PUBLIC' };
const secondary = { credential: 'ref:read-public', signing: 'ref:read-signing', publicCredential: 'ed25519:READ' };
const signature = '0a'.repeat(64);
const remote = { recipe: 'orderly.rest.1', inputs: { timestamp: '1', method: 'GET', path: '/v1/positions' } };
const signal = () => new AbortController().signal;

const signer = (requests: RemoteSignerRequest[], recipes: readonly string[] | undefined,
  answer: (request: RemoteSignerRequest) => RemoteSignerResponse = (request) => ({ id: request.id, opSignature: signature })): RemoteSigner => ({
  accountName: 'owned-account',
  ...(recipes === undefined ? {} : { signOps: { recipes } }),
  sign: async (request) => { requests.push(request); return answer(request); },
  isAvailable: () => true,
  onAvailabilityChange: () => () => {},
});
const work = (signing = primary.signing, present = true): SignRequest => ({
  receipt: { stampMs: 1 },
  context: { venue: 'advisory', method: 'advisory', host: '', path: '' },
  operations: [
    ...(present ? [{ kind: 'present' as const, key: primary.credential }] : []),
    { kind: 'sign', key: signing, algorithm: 'ed25519', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode('1GET/v1/positions')) }] },
  ],
});
const provider = (remoteSigner?: RemoteSigner, payloadSigner?: PayloadSigner) => createRemoteSignerProvider({
  exchangeName: 'orderly', identities: { primary, secondary }, remoteSigner, payloadSigner,
});

it('an advertised recipe crosses as one exact v2 op and returns the raw signature in place', async () => {
  const requests: RemoteSignerRequest[] = [];
  const result = await provider(signer(requests, ['paradex.order.1', 'orderly.rest.1']))(work(), { signal: signal(), remote });
  expect(JSON.stringify(requests)).toBe(JSON.stringify([{
    id: 'fixed-op-id', type: 'op', v: 2, exchangeName: 'orderly', accountName: 'owned-account',
    op: { recipe: 'orderly.rest.1', inputs: remote.inputs },
  }]));
  expect(result.values).toEqual([Array.from(new TextEncoder().encode('ed25519:PUBLIC')), Array(64).fill(10)]);
  expect(result.receipt).toEqual({ stampMs: 1 });
});

it('unadvertised, version-skewed or absent capabilities are refused before any request', async () => {
  for (const recipes of [undefined, [], ['orderly.rest.2'], ['orderly.rest'], ['ORDERLY.REST.1']]) {
    const requests: RemoteSignerRequest[] = [];
    await expect(provider(signer(requests, recipes))(work(), { signal: signal(), remote })).rejects.toThrow('refused');
    expect(requests).toEqual([]);
  }
});

it('secondary keys, the payload-signer path and malformed replies are refused', async () => {
  const requests: RemoteSignerRequest[] = [];
  await expect(provider(signer(requests, ['orderly.rest.1']))(work(secondary.signing, false), { signal: signal(), remote }))
    .rejects.toThrow('refused');
  expect(requests).toEqual([]);
  await expect(provider(signer(requests, ['orderly.rest.1']))(work(), { signal: signal(), remote: { ...remote, role: 'zklinkSeed' } }))
    .rejects.toThrow('refused');
  expect(requests).toEqual([]);
  const payloadSigner = vi.fn<PayloadSigner>(() => ({ signature: 'x' }));
  await expect(provider(undefined, payloadSigner)(work(), { signal: signal(), remote })).rejects.toThrow('refused');
  expect(payloadSigner).not.toHaveBeenCalled();
  for (const reply of [
    (request: RemoteSignerRequest) => ({ id: request.id, opSignature: 'AB'.repeat(64) }),
    (request: RemoteSignerRequest) => ({ id: request.id, opSignature: 'abc' }),
    (request: RemoteSignerRequest) => ({ id: request.id, wsAuthSignature: signature }),
    (request: RemoteSignerRequest) => ({ id: request.id, opSignature: signature, error: 'denied' }),
    () => ({ id: 'other-id', opSignature: signature }),
  ]) {
    await expect(provider(signer([], ['orderly.rest.1'], reply))(work(), { signal: signal(), remote })).rejects.toThrow('refused');
  }
});

it('work without a bound recipe keeps the unchanged HMAC ws-auth wire', async () => {
  const requests: RemoteSignerRequest[] = [];
  const hmac = createRemoteSignerProvider({
    exchangeName: 'bybit', identities: { primary: { ...primary, publicCredential: 'public' } },
    remoteSigner: signer(requests, ['orderly.rest.1'], (request) => ({ id: request.id, wsAuthSignature: 'ab'.repeat(32) })),
  });
  const request = work();
  request.operations[1] = { kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: [49] }] };
  await hmac(request, { signal: signal() });
  expect(requests).toHaveLength(1);
  expect(requests[0].type).toBe('ws-auth');
  expect(requests[0]).not.toHaveProperty('v');
  expect(requests[0]).not.toHaveProperty('op');
});
