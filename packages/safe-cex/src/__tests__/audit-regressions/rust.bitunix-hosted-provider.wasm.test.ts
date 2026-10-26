import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSigner, RemoteSignerRequest } from '../../types';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { createHash } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

const STAMP = 1_700_000_000_000;
const KEY = 'api-key-xyz';
const MATERIAL = 'test-secret-value';
// Independent Node crypto oracle of `sha256(sha256(toSign) ‖ material)` in hex.
const doubleSha256 = (toSign: string) => createHash('sha256')
  .update(createHash('sha256').update(toSign).digest('hex') + MATERIAL).digest('hex');
const seeded = () => { let next = 0; return (length: number) => Uint8Array.from({ length }, () => next++ & 255); };
const live: { dispose(): void }[] = [];
afterEach(() => {
  live.splice(0).forEach((item) => item.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport();
});

const transport = (sent: Parameters<Transport['request']>[0][]): Transport => ({
  now: () => STAMP, prepareHttp: prepareRecordedHttp,
  request: async (request) => {
    sent.push(request);
    return { status: 200, headers: '', body: new TextEncoder().encode('{"code":0,"data":{"orderList":[]}}') };
  },
  connect: () => { throw new Error('unexpected socket'); },
  setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
});

/** The keyed browser: an actual WASM engine answering through its hosted capture. */
const browser = () => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('Signing capture must not use network'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => STAMP, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange('bitunix', { engine: 'rust', key: KEY, secret: MATERIAL }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
};

const history = async (engine: unknown, options: ConstructorParameters<typeof EngineHost>[3]) => {
  const sent: Parameters<Transport['request']>[0][] = [];
  const host = new EngineHost(engine as never, transport(sent), {}, { ownsEngine: true, signingEntropy: seeded(), ...options });
  live.push(host);
  await host.command({ op: 'fetchOrderHistory', symbol: 'BTCUSDT' });
  return sent[0];
};

it('actual WASM keyed Bitunix hosted capture signs the TS login and REST goldens', async () => {
  const { exchange, io } = browser();
  // Node crypto vector from base.ws-auth-signing.test.ts.
  await expect(exchange.signWsAuth({ id: 'login', type: 'ws-auth', exchangeName: 'bitunix',
    wsAuth: { toSign: 'nonce-abc1700000000api-key-xyz', outputFormat: 'bitunix-double-sha256' } }))
    .resolves.toEqual({ id: 'login', wsAuthSignature: 'c99d3e027fcb8cbefc05f7a38dfc710011d07e79dbefd8b0bc7b44347bb0e624' });
  await expect(exchange.signWsAuth({ id: 'hmac', type: 'ws-auth', exchangeName: 'bitunix',
    wsAuth: { toSign: 'nonce-abc1700000000api-key-xyz', outputFormat: 'hex' } })).resolves.toMatchObject({ error: expect.any(String) });
  const signed = await exchange.signHttpRequest({ id: 'rest', type: 'http', exchangeName: 'bitunix',
    config: { method: 'POST', url: '/api/v1/futures/trade/place_order', baseURL: 'https://fixture.invalid',
      data: { symbol: 'BTCUSDT', qty: '1' }, headers: {} } });
  expect(signed.error).toBeUndefined();
  const headers = signed.config!.headers!;
  expect(headers).toMatchObject({ 'api-key': KEY, timestamp: String(STAMP) });
  expect(headers.sign).toBe(doubleSha256(`${headers.nonce}${STAMP}${KEY}{"symbol":"BTCUSDT","qty":"1"}`));
  expect(io).not.toHaveBeenCalled(); expect(exchange.hasStarted).toBe(false);
});

it('actual WASM keyless Bitunix dispatch sends the local-key request bytes through the browser wire', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const local = await history(new WasmEngine('bitunix', JSON.stringify({ key: KEY, secret: MATERIAL })), {});

  const keyed = browser();
  const sign = vi.fn((request: RemoteSignerRequest) => keyed.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const identity = { credential: 'delegated/primary/credential', signing: 'delegated/primary/signing' };
  const signer = createRemoteSignerProvider({ exchangeName: 'bitunix', remoteSigner,
    identities: { primary: { ...identity, publicCredential: KEY } } });
  const remote = await history(new WasmEngine('bitunix', JSON.stringify({ signing: { primary: identity }, remoteSigning: true })),
    { signer, signerAlgorithms: remoteSignerAlgorithms('bitunix') });

  expect(local.headers.sign).toMatch(/^[0-9a-f]{64}$/);
  expect(remote.url).toBe(local.url);
  expect(remote.headers).toEqual(local.headers);
  expect(remote.body).toEqual(local.body);
  const query = new URL(local.url).search.slice(1);
  const toSign = `${local.headers.nonce}${STAMP}${KEY}${decodeURIComponent(query).split('&').sort().join('').replace(/=/g, '')}`;
  expect(sign).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'bitunix',
    accountName: 'hosted-account', wsAuth: { toSign, outputFormat: 'bitunix-double-sha256' } }));
  expect(local.headers.sign).toBe(doubleSha256(toSign));
  expect(keyed.io).not.toHaveBeenCalled();
});
