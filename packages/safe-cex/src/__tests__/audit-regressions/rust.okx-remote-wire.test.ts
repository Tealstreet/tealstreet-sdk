import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSigner, RemoteSignerRequest } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { OKXPrivateWebsocket } from '../../exchanges/okx/okx.ws-private';
import { OKXBusinessWebsocket } from '../../exchanges/okx/okx.ws-business';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { virtualClock } from '../../utils/virtualClock';

vi.mock('nanoid', () => ({ nanoid: () => 'fixed-wire-id' }));
const now = 1_700_000_000_123;
const seconds = 1_700_000_000;
const identity = { credential: 'ref:owned-public', signing: 'ref:owned-primary', publicCredential: 'public+é',
  passphrase: 'ref:owned-passphrase', passphraseValue: 'fixture-presentation' };
const encode = (text: string) => ({ kind: 'bytes' as const, bytes: Array.from(new TextEncoder().encode(text)) });
const prepared = (native: boolean, stampMs: number | undefined = seconds * 1000): SignRequest => ({
  receipt: stampMs === undefined ? {} : { stampMs }, context: { venue: 'advisory', method: 'advisory', host: '', path: '' },
  operations: [
    { kind: 'present', key: identity.credential },
    { kind: 'sign', key: identity.signing, algorithm: 'hmacSha256',
      payload: native ? [encode(String(seconds)), encode('GET'), encode('/users/self/verify')]
        : [encode(`${seconds}GET/users/self/verify`)] },
    { kind: 'present', key: identity.passphrase },
  ],
});
const remote = (requests: RemoteSignerRequest[]): RemoteSigner => ({ accountName: 'owned-account',
  sign: async (request) => { requests.push(request); return { id: request.id, wsAuthSignature: Buffer.alloc(32, 7).toString('base64') }; },
  isAvailable: () => true, onAvailabilityChange: () => () => {} });
afterEach(() => vi.restoreAllMocks());

for (const Socket of [OKXPrivateWebsocket, OKXBusinessWebsocket]) {
  it.each(['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const)(`%s ${Socket.name} has byte-identical legacy RemoteSigner envelope`, async (exchangeName) => {
    vi.spyOn(virtualClock, 'getCurrentTime').mockReturnValue({ unix: () => seconds } as ReturnType<typeof virtualClock.getCurrentTime>);
    const legacy: RemoteSignerRequest[] = []; const actual: RemoteSignerRequest[] = [];
    const parent = { name: exchangeName, region: exchangeName.startsWith('okxeu') ? 'okxeu' : 'okx',
      options: { key: identity.publicCredential, passphrase: identity.passphraseValue, remoteSigner: remote(legacy) } };
    const socket = new Socket(parent as unknown as ConstructorParameters<typeof OKXPrivateWebsocket>[0]);
    socket.sendWithLog = vi.fn();
    await socket.auth();
    const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual) });
    await provider(prepared(true), { signal: new AbortController().signal });
    expect(legacy).toHaveLength(1); expect(actual).toHaveLength(1);
    expect(JSON.stringify(actual[0])).toBe(JSON.stringify(legacy[0]));
    expect(actual[0].wsAuth?.extraData).toEqual({ timestamp: seconds });
  });
}

it('opaque HTTP work cannot acquire WS metadata, even when its bytes match the WS preimage', async () => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'okx', identities: { primary: identity }, remoteSigner: remote(actual) });
  await provider(prepared(false), { signal: new AbortController().signal });
  expect(actual[0].wsAuth).not.toHaveProperty('extraData');
});

it.each([now, seconds * 1000 + 1])('contradictory native WS receipt %s refuses before callback', async (stampMs) => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'okx', identities: { primary: identity }, remoteSigner: remote(actual) });
  await expect(provider(prepared(true, stampMs), { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  expect(actual).toEqual([]);
});

it('an unrelated family retains valid UTF-8 split across primitive payload parts', async () => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'binance', identities: { primary: {
    credential: identity.credential, signing: identity.signing, publicCredential: identity.publicCredential,
  } }, remoteSigner: { ...remote(actual), sign: async (request) => {
    actual.push(request); return { id: request.id, wsAuthSignature: 'ab'.repeat(32) };
  } } });
  const work = prepared(false);
  work.operations = [{ kind: 'present', key: identity.credential }, { kind: 'sign', algorithm: 'hmacSha256', key: identity.signing,
    payload: [{ kind: 'bytes', bytes: [65] }, { kind: 'bytes', bytes: [0xc3] }, { kind: 'bytes', bytes: [0xa9] }] }];
  await provider(work, { signal: new AbortController().signal });
  expect(actual[0].wsAuth?.toSign).toBe('Aé');
});
