import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSigner, RemoteSignerRequest } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { BitgetPrivateWebsocket } from '../reference/bitget/bitget.ws-private';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { virtualClock } from '../../utils/virtualClock';

vi.mock('nanoid', () => ({ nanoid: () => 'fixed-wire-id' }));
const aliases = ['bitget', 'bitgetusdc', 'bitgetinverse'] as const;
const seconds = 1_700_000_000;
const identity = { credential: 'ref:owned-public', signing: 'ref:owned-primary', publicCredential: 'public+é',
  passphrase: 'ref:owned-passphrase', passphraseValue: 'fixture-presentation' };
const secondary = { credential: 'ref:owned-read-public', signing: 'ref:owned-read', publicCredential: 'read-public',
  passphrase: 'ref:owned-passphrase', passphraseValue: 'fixture-presentation' };
const encode = (text: string) => ({ kind: 'bytes' as const, bytes: Array.from(new TextEncoder().encode(text)) });
const login = (parts: string[], stampMs = seconds * 1000, signing = identity.signing): SignRequest => ({
  receipt: { stampMs }, context: { venue: 'advisory', method: 'advisory', host: '', path: '' },
  operations: [
    { kind: 'present', key: identity.credential },
    { kind: 'sign', key: signing, algorithm: 'hmacSha256', payload: parts.map(encode) },
    { kind: 'present', key: identity.passphrase },
  ],
});
const native = (stampMs = seconds * 1000) => login([String(seconds), 'GET', '/user/verify'], stampMs);
const remote = (requests: RemoteSignerRequest[]): RemoteSigner => ({ accountName: 'owned-account',
  sign: async (request) => { requests.push(request); return { id: request.id, wsAuthSignature: Buffer.alloc(32, 7).toString('base64') }; },
  isAvailable: () => true, onAvailabilityChange: () => () => {} });
const signal = () => ({ signal: new AbortController().signal });
afterEach(() => vi.restoreAllMocks());

it.each(aliases)('%s private login has a byte-identical legacy RemoteSigner envelope', async (exchangeName) => {
  vi.spyOn(virtualClock, 'getCurrentTime').mockReturnValue({ unix: () => seconds } as ReturnType<typeof virtualClock.getCurrentTime>);
  const legacy: RemoteSignerRequest[] = []; const actual: RemoteSignerRequest[] = [];
  const parent = { name: exchangeName, options: { key: identity.publicCredential, passphrase: identity.passphraseValue, remoteSigner: remote(legacy) } };
  const socket = new BitgetPrivateWebsocket(parent as unknown as ConstructorParameters<typeof BitgetPrivateWebsocket>[0]);
  socket.sendWithLog = vi.fn();
  await (socket as unknown as { auth(): Promise<void> }).auth();
  const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual) });
  const result = await provider(native(), signal());
  expect(legacy).toHaveLength(1); expect(actual).toHaveLength(1);
  expect(JSON.stringify(actual[0])).toBe(JSON.stringify(legacy[0]));
  expect(actual[0]).toMatchObject({ exchangeName: 'bitget', wsAuth: { toSign: `${seconds}GET/user/verify`, outputFormat: 'base64',
    extraData: { timestamp: seconds } } });
  expect(result.values[2]).toEqual(Array.from(new TextEncoder().encode('fixture-presentation')));
});

it.each(aliases)('%s opaque HTTP work keeps its alias label and cannot acquire WS metadata', async (exchangeName) => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual) });
  await provider(login([`${seconds}GET/user/verify`]), signal());
  expect(actual[0]).toMatchObject({ exchangeName, wsAuth: { toSign: `${seconds}GET/user/verify`, outputFormat: 'base64' } });
  expect(actual[0].wsAuth).not.toHaveProperty('extraData');
});

it.each([seconds * 1000 + 1, (seconds + 1) * 1000])('contradictory native Bitget receipt %s refuses before callback', async (stampMs) => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'bitget', identities: { primary: identity }, remoteSigner: remote(actual) });
  await expect(provider(native(stampMs), signal())).rejects.toThrow('Remote signing request refused');
  expect(actual).toEqual([]);
});

it.each([
  ['bitget', [String(seconds), 'GET', '/users/self/verify']],
  ['okx', [String(seconds), 'GET', '/user/verify']],
  ['bitget', [String(seconds), 'get', '/user/verify']],
] as const)('%s decodes only its own declared seconds suffix %j', async (exchangeName, parts) => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual) });
  await provider(login([...parts]), signal());
  expect(actual[0].wsAuth).not.toHaveProperty('extraData');
});

it('a hosted single-part Bitget login capture keeps legacy seconds metadata', async () => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'bitgetusdc', identities: { primary: identity }, remoteSigner: remote(actual) });
  const capture = login([`${seconds}GET/user/verify`]);
  capture.receipt = {};
  await provider(capture, signal());
  expect(actual[0]).toMatchObject({ exchangeName: 'bitget', wsAuth: { extraData: { timestamp: seconds } } });
});

it.each(aliases)('%s primary-only remote wire refuses required-secondary work before callback', async (exchangeName) => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity, secondary }, remoteSigner: remote(actual) });
  const work = login([`${seconds * 1000}GET/api/v2/mix/account/accounts`], seconds * 1000, secondary.signing);
  work.operations[0] = { kind: 'present', key: secondary.credential };
  await expect(provider(work, signal())).rejects.toThrow('Remote signing request refused');
  expect(actual).toEqual([]);
});

it.each(aliases)('%s RemoteSigner wins over a supplied PayloadSigner without fallback', async (exchangeName) => {
  const actual: RemoteSignerRequest[] = [];
  const payloadSigner = vi.fn(() => { throw new Error('No payload fallback'); });
  const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual), payloadSigner });
  await provider(native(), signal());
  expect(actual).toHaveLength(1);
  expect(payloadSigner).not.toHaveBeenCalled();
});
