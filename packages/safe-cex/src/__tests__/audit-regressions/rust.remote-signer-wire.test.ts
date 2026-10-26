import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSigner, RemoteSignerRequest } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';
import { BybitPrivateWebsocket } from '../reference/bybit/bybit.ws-private';
import { BinancePrivateWebsocket } from '../reference/binance/binancespot.ws-private';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { virtualClock } from '../../utils/virtualClock';

vi.mock('nanoid', () => ({ nanoid: () => 'fixed-wire-id' }));
const NOW = 1_700_000_000_123;
const identity = { credential: 'ref:owned-public', signing: 'ref:owned-primary', publicCredential: 'public+é' };
const mac = 'ab'.repeat(32);
const remote = (requests: RemoteSignerRequest[]): RemoteSigner => ({ accountName: 'owned-account',
  sign: async (request) => { requests.push(request); return { id: request.id, wsAuthSignature: mac }; },
  isAvailable: () => true, onAvailabilityChange: () => () => {} });
afterEach(() => vi.restoreAllMocks());
const prepared = (text: string, receipt: SignRequest['receipt']): SignRequest => ({ receipt,
  context: { venue: 'advisory', method: 'advisory', host: '', path: '' }, operations: [
    { kind: 'present', key: identity.credential },
    { kind: 'sign', key: identity.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(text)) }] },
  ] });

it.each(['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const)(
  '%s emits the exact serialized legacy Bybit WS envelope, including label and expiry', async (name) => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
    const legacy: RemoteSignerRequest[] = []; const actual: RemoteSignerRequest[] = [];
    const parent = { name, options: { key: identity.publicCredential, remoteSigner: remote(legacy) } };
    const socket = new BybitPrivateWebsocket(parent as unknown as ConstructorParameters<typeof BybitPrivateWebsocket>[0]);
    socket.sendWithLog = vi.fn();
    await (socket as unknown as { auth(): Promise<boolean> }).auth();
    const provider = createRemoteSignerProvider({ exchangeName: name, identities: { primary: identity }, remoteSigner: remote(actual) });
    await provider(prepared(`GET/realtime${NOW + 10000}`, { expiryMs: NOW + 10000 }), { signal: new AbortController().signal });
    expect(actual).toHaveLength(1); expect(legacy).toHaveLength(1);
    expect(JSON.stringify(actual[0])).toBe(JSON.stringify(legacy[0]));
  },
);

it('Binance Spot emits its exact legacy WS envelope, fixed id and prepared public metadata', async () => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
  const legacy: RemoteSignerRequest[] = []; const actual: RemoteSignerRequest[] = [];
  const parent = { name: 'binancespot', options: { key: identity.publicCredential, remoteSigner: remote(legacy) } };
  const socket = new BinancePrivateWebsocket(parent as unknown as ConstructorParameters<typeof BinancePrivateWebsocket>[0]);
  await (socket as unknown as { buildUserDataSubscriptionParams(): Promise<unknown> }).buildUserDataSubscriptionParams();
  const provider = createRemoteSignerProvider({ exchangeName: 'binancespot', identities: { primary: identity }, remoteSigner: remote(actual) });
  const work = prepared('', { stampMs: NOW });
  work.operations[1] = { kind: 'sign', key: identity.signing, algorithm: 'hmacSha256', payload: [
    { kind: 'bytes', bytes: Array.from(new TextEncoder().encode('apiKey=')) },
    { kind: 'credential', key: identity.credential },
    { kind: 'bytes', bytes: Array.from(new TextEncoder().encode(`&recvWindow=5000&timestamp=${NOW}`)) },
  ] };
  await provider(work, { signal: new AbortController().signal });
  expect(JSON.stringify(actual[0])).toBe(JSON.stringify(legacy[0]));
});

it('HTTP work cannot acquire WS metadata or a fixed correlation id from advisory context', async () => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'binancespot', identities: { primary: identity }, remoteSigner: remote(actual) });
  const work = prepared(`symbol=BTCUSDT&timestamp=${NOW}`, { stampMs: NOW });
  work.context = { venue: 'binancespot', method: 'WS', host: 'advisory', path: 'userDataStream.subscribe.signature' };
  await provider(work, { signal: new AbortController().signal });
  expect(actual[0].id).toBe('fixed-wire-id');
  expect(actual[0].wsAuth).not.toHaveProperty('extraData');
});

it.each(['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const)(
  '%s HTTP primitive envelopes keep their original alias without WS metadata', async (exchangeName) => {
    const actual: RemoteSignerRequest[] = [];
    const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual) });
    await provider(prepared(`1000${identity.publicCredential}20000symbol=BTCUSDT`, { stampMs: 1000 }), { signal: new AbortController().signal });
    expect(actual[0].exchangeName).toBe(exchangeName);
    expect(actual[0].wsAuth).not.toHaveProperty('extraData');
    expect(actual[0].id).toBe('fixed-wire-id');
  },
);

it.each([
  ['bybit', `GET/realtime${NOW + 10000}`, { expiryMs: NOW + 10001 }],
  ['bybit', 'GET/realtimeNaN', {}],
  ['binancespot', `apiKey=${identity.publicCredential}&recvWindow=5000&timestamp=NaN`, {}],
] as const)('%s refuses recognized WS grammar with malformed or contradictory prepared metadata', async (exchangeName, text, receipt) => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: remote(actual) });
  await expect(provider(prepared(text, receipt), { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  expect(actual).toEqual([]);
});

it('Binance Spot HTTP bytes matching WS grammar keep HTTP identity and no WS metadata', async () => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'binancespot', identities: { primary: identity }, remoteSigner: remote(actual) });
  await provider(prepared(`apiKey=${identity.publicCredential}&recvWindow=5000&timestamp=${NOW}`, { stampMs: NOW }), { signal: new AbortController().signal });
  expect(actual[0].id).toBe('fixed-wire-id');
  expect(actual[0].wsAuth).not.toHaveProperty('extraData');
});

it('native Spot WS metadata mismatch and later malformed WS work reject the whole batch before callbacks', async () => {
  const actual: RemoteSignerRequest[] = [];
  const provider = createRemoteSignerProvider({ exchangeName: 'binancespot', identities: { primary: identity }, remoteSigner: remote(actual) });
  const work = prepared('first-valid-preimage', { stampMs: NOW });
  work.operations.push({ kind: 'sign', key: identity.signing, algorithm: 'hmacSha256', payload: [
    { kind: 'bytes', bytes: Array.from(new TextEncoder().encode('apiKey=')) },
    { kind: 'credential', key: identity.credential },
    { kind: 'bytes', bytes: Array.from(new TextEncoder().encode(`&recvWindow=5000&timestamp=${NOW + 1}`)) },
  ] });
  await expect(provider(work, { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  expect(actual).toEqual([]);
  const bybit = createRemoteSignerProvider({ exchangeName: 'bybit', identities: { primary: identity }, remoteSigner: remote(actual) });
  const batch = prepared('first-valid-preimage', {});
  batch.operations.push(...prepared('GET/realtimeNaN', {}).operations.slice(1));
  await expect(bybit(batch, { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  expect(actual).toEqual([]);
});
