import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { sha256 } from '@noble/hashes/sha2.js';
import { afterEach, describe, expect, it, vi } from 'vitest';

import vectors from '../../../../../sdk-rs/venues/paradex/tests/sign-vectors.json';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

type Case = (typeof vectors.cases)[number];
const IDENTITY = { credential: 'delegated/primary/credential', signing: 'delegated/primary/signing' };
const live: { dispose(): void }[] = [];
afterEach(() => {
  live.splice(0).forEach((item) => item.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});

const transport = (now: number, io: () => never) => ({ prepareHttp: prepareRecordedHttp, now: () => now, request: io, connect: io,
  setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
  clearTimeout: (timer: unknown) => clearTimeout(timer as ReturnType<typeof setTimeout>) });

/** The keyed device: an actual WASM engine holding the vector's Stark key. */
const device = (vector: Case, options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn((): never => { throw new Error('Signing capture must not use network'); });
  setRustEngineTransport(() => transport(vector.now, io));
  const exchange = new RustEngineExchange('paradex', { engine: 'rust', key: vector.key, secret: '', paradexAddress: vector.address,
    subkeyAddress: vector.subkey, testnet: vector.environment === 'testnet', ...options }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
};

/** The server: the keyless provider, wired to the device over the ws-auth wire. */
const server = (vector: Case, keyed: ReturnType<typeof device>) => {
  const sign = vi.fn((request: RemoteSignerRequest) => keyed.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const signer = createRemoteSignerProvider({ exchangeName: 'paradex', remoteSigner,
    identities: { primary: { ...IDENTITY, publicCredential: vector.address } } });
  return { signer, sign };
};

const call = (signer: ReturnType<typeof server>['signer'], request: SignRequest) =>
  Promise.resolve(signer(request, { signal: new AbortController().signal }));
const context = { venue: 'paradex', method: 'POST', host: 'api.prod.paradex.trade', path: '/v1/orders' };
const bindKey = { receipt: {}, context, operations: [{ kind: 'publicKey', algorithm: 'starkDigest', key: IDENTITY.signing }] } as unknown as SignRequest;
/** The engine's shape: one statement commitment, then one Stark signature per digest. */
const work = (statement: string, signatures: number): SignRequest => ({
  receipt: {},
  context,
  operations: [
    { kind: 'sha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(statement)) }] },
    ...Array.from({ length: signatures }, () => ({ kind: 'sign', algorithm: 'starkDigest', key: IDENTITY.signing,
      payload: [{ kind: 'bytes', bytes: new Array(32).fill(1) }] })),
  ],
} as unknown as SignRequest);

const tagged = (op: string, body: object) => JSON.stringify({ family: 'paradex', op, revision: 'paradex/1', ...body });
const auth = (vector: Case) => tagged('auth', { timestampS: Math.floor(vector.now / 1000), expirationS: Math.floor(vector.now / 1000) + 300 });
/** A vector order in its native projection: only its hashed fields come from the
 * vector, with sizes in native `num::format` spelling (`1e-8` is `0.00000001`). */
const projected = (vector: Case, order: Case['orders'][number], clientId: string | null) => {
  const stop = order.type === 'STOP_MARKET';
  return { timestampMs: vector.now, market: order.market, side: order.side, orderType: order.type,
    size: order.size === '1e-8' ? '0.00000001' : order.size,
    price: order.type === 'LIMIT' ? order.price : null, triggerPrice: stop ? '60000' : null, instruction: 'GTC',
    flags: stop ? ['REDUCE_ONLY'] : null, stp: stop || clientId === null ? null : 'EXPIRE_TAKER', clientId: stop ? null : clientId };
};
const named = (vector: Case, name: string) => vector.orders.find((order) => order.name === name)!;
/** starknet.js `["r","s"]` decimal felts as the provider's 64-byte `r || s`. */
const rs = (signature: string) => (JSON.parse(signature) as string[])
  .flatMap((felt) => Array.from(Buffer.from(BigInt(felt).toString(16).padStart(64, '0'), 'hex')));
const commitment = (statement: string) => Array.from(sha256(new TextEncoder().encode(statement)));

describe('Paradex remote signing over the structured ws-auth wire', () => {
  it('admits a keyless remoteSigner account and still refuses PayloadSigner', () => {
    const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} } as unknown as RemoteSigner;
    const account = { key: '0x0123', paradexAddress: '0x0123', subkeyAddress: '0x456' };
    expect(() => assertBetaAccount('paradex', { ...account, remoteSigner })).not.toThrow();
    expect(remoteSignerAlgorithms('paradex')).toEqual(['starkDigest']);
    const exchange = new RustEngineExchange('paradex', { engine: 'rust', ...account, secret: '', remoteSigner }, new DefaultStore());
    live.push(exchange);
    expect(exchange.isPublicDataOnly).toBe(false);
    expect(() => new RustEngineExchange('paradex', { engine: 'rust', ...account, secret: '', payloadSigner: vi.fn() }, new DefaultStore()))
      .toThrow(/PayloadSigner/);
  });

  it.each(vectors.cases.map((vector) => [vector.environment, vector] as const))('%s: device signatures equal independent starknet.js signatures', async (_, vector) => {
    const keyed = device(vector);
    const { signer, sign } = server(vector, keyed);
    const [key] = (await call(signer, bindKey)).values;
    expect(key).toHaveLength(32);
    expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'paradex', accountName: 'hosted-account',
      wsAuth: { toSign: vector.address, outputFormat: 'paradex-public' } }));
    const create = (name: string) => [name, tagged('orderCreate', { target: 'orders',
      order: projected(vector, named(vector, name), `c-${name}`) }), named(vector, name).signature] as [string, string, string];
    const modify = named(vector, 'modify') as Case['orders'][number] & { id: string };
    const cases: [string, string, string][] = [
      ['auth', auth(vector), vector.shortAuth.signature],
      create('limit-tiny'), create('market'), create('stop-market'),
      ['modify', tagged('orderModify', { id: modify.id, timestampMs: vector.now, market: modify.market, side: modify.side,
        orderType: modify.type, size: modify.size, price: modify.price, triggerPrice: null }), modify.signature],
    ];
    for (const [name, statement, expected] of cases) {
      const result = await call(signer, work(statement, 1));
      expect(result.values, name).toEqual([commitment(statement), rs(expected)]);
      expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ wsAuth: { toSign: statement, outputFormat: 'paradex-stark' } }));
    }
    // A native batch: one commitment, every item signed in order.
    const limit = named(vector, 'limit-tiny');
    const batch = tagged('orderBatch', { items: ['a', 'b', 'c'].map((id) => projected(vector, limit, id)) });
    const result = await call(signer, work(batch, 3));
    expect(result.values).toEqual([commitment(batch), rs(limit.signature), rs(limit.signature), rs(limit.signature)]);
    expect(keyed.io).not.toHaveBeenCalled();
  });

  it('the device refuses stale, malformed, foreign-account and bare-digest requests', async () => {
    const vector = vectors.cases[0];
    const keyed = device(vector);
    const { signer } = server(vector, keyed);
    await call(signer, bindKey);
    const stale = tagged('auth', { timestampS: Math.floor(vector.now / 1000) - 31, expirationS: Math.floor(vector.now / 1000) + 269 });
    const order = projected(vector, vector.orders[0], 'c0');
    for (const [name, statement] of [
      ['stale', stale],
      ['digest', vector.orders[0].hash],
      ['extra', tagged('orderCreate', { target: 'orders', order: { ...order, digest: '0x1' } })],
      ['transfer', tagged('transfer', { amount: '1' })],
      ['market on batch', tagged('orderCreate', { target: 'ordersBatchSingle', order: projected(vector, vector.orders[1], 'c1') })],
    ]) {
      await expect(call(signer, work(statement, 1)), name).rejects.toThrow('Remote signing request refused');
      await expect(keyed.exchange.signWsAuth({ id: name, type: 'ws-auth', exchangeName: 'paradex',
        wsAuth: { toSign: statement, outputFormat: 'paradex-stark' } })).resolves.toMatchObject({ error: expect.any(String) });
    }
    // The key binding answers only for the signed-in account.
    await expect(keyed.exchange.signWsAuth({ id: 'other', type: 'ws-auth', exchangeName: 'paradex',
      wsAuth: { toSign: '0x0124', outputFormat: 'paradex-public' } })).resolves.toMatchObject({ error: expect.any(String) });
    // An HMAC or wallet format never reaches the Stark key.
    for (const outputFormat of ['hex', 'hyperliquid-l1'] as const) {
      await expect(keyed.exchange.signWsAuth({ id: outputFormat, type: 'ws-auth', exchangeName: 'paradex',
        wsAuth: { toSign: auth(vector), outputFormat } })).resolves.toMatchObject({ error: expect.any(String) });
    }
  });

  it('the server never signs before binding a key, nor adopts a second device key', async () => {
    const vector = vectors.cases[0];
    const keyed = device(vector);
    const { signer } = server(vector, keyed);
    await expect(call(signer, work(auth(vector), 1))).rejects.toThrow('Remote signing request refused');
    const other = device(vector, { key: `0x${'0'.repeat(63)}2` });
    const rebound = createRemoteSignerProvider({ exchangeName: 'paradex',
      remoteSigner: { accountName: 'hosted-account', isAvailable: () => true, onAvailabilityChange: () => () => {},
        sign: vi.fn().mockImplementationOnce((request: RemoteSignerRequest) => keyed.exchange.signWsAuth(request))
          .mockImplementation((request: RemoteSignerRequest) => other.exchange.signWsAuth(request)) },
      identities: { primary: { ...IDENTITY, publicCredential: vector.address } } });
    await call(rebound, bindKey);
    await expect(call(rebound, bindKey)).rejects.toThrow('Remote signing request refused');
  });
});

describe('keyless Paradex engine binds its Stark key through the device', () => {
  it('runs the engine PublicKey setup task over paradex-public before start', async () => {
    const { EngineHost } = await import('@tealstreet/safe-cex-wasm/host');
    const { WasmEngine } = await loadNodeEngine();
    const vector = vectors.cases[0];
    const keyed = device(vector);
    const { signer, sign } = server(vector, keyed);
    const engine = new WasmEngine('paradex', JSON.stringify({ paradexAddress: vector.address, subkeyAddress: vector.subkey,
      signing: { primary: IDENTITY }, remoteSigning: true }));
    const io = vi.fn((): never => { throw new Error('setup must not use network'); });
    const host = new EngineHost(engine as never, transport(vector.now, io), {},
      { ownsEngine: true, signer, signerAlgorithms: remoteSignerAlgorithms('paradex') });
    live.push(host);
    await host.prepareAddressSetup();
    expect(sign).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ wsAuth: { toSign: vector.address, outputFormat: 'paradex-public' } }));
    expect((engine as unknown as { addressSetupReady(): boolean }).addressSetupReady()).toBe(true);
    expect(io).not.toHaveBeenCalled();
  });
});
