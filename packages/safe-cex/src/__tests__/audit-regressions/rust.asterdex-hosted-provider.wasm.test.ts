import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHash, createHmac } from 'node:crypto';
import { Wallet, getBytes } from 'ethers';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

import golden from '../../../../../sdk-rs/venues/asterdex/tests/fixtures/signing.json';
const corpus = golden.cases;
let stamp = 1_748_310_859_508;
const live: RustEngineExchange[] = [];
const primary = { credential: 'origin/key', signing: 'origin/sign', publicCredential: 'fixture-key' };
vi.mock('../../secondaryReadKeys', () => ({ SECONDARY_READ_KEYS_ENABLED: true }));
afterEach(() => { live.splice(0).forEach((exchange) => exchange.dispose()); setWasmEngineLoader(loadNodeEngine); setRustEngineTransport(); });
function device(options: Partial<ExchangeOptions>) {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('No network in hosted fixture'); });
  setRustEngineTransport(() => ({ now: () => stamp, prepareHttp: prepareRecordedHttp, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange('asterdex', { key: 'fixture-key', secret: 'fixture-material', ...options } as ExchangeOptions, new DefaultStore());
  live.push(exchange);
  const logs: string[] = []; exchange.emitter.on('log', (message: string) => logs.push(message));
  const sign = vi.fn((request: RemoteSignerRequest) => exchange.signWsAuth(request));
  return { exchange, sign, io, logs, remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } };
}

it('actual WASM V1 keyless origin matches local HTTP signing at fixed and corrected clocks', async () => {
  for (stamp of [1_748_310_859_508, 1_748_310_869_508]) {
    const browser = device({});
    const server = device({ secret: '', remoteSigner: browser.remoteSigner });
    for (const [method, path] of [['get', '/fapi/v4/account'], ['get', '/fapi/v1/openOrders'], ['post', '/fapi/v1/order'],
      ['delete', '/fapi/v1/order'], ['post', '/fapi/v1/listenKey'], ['put', '/fapi/v1/listenKey']]) {
      const input: RemoteSignerRequest = { id: 'fixture', type: 'http', exchangeName: 'asterdex',
        config: { method, url: path, baseURL: 'https://fapi.asterdex.com', headers: {}, params: { symbol: 'BTCUSDT' } } };
      const local = await browser.exchange.signHttpRequest(input);
      expect(local.error).toBeUndefined();
      expect(await server.exchange.signHttpRequest(input)).toEqual(local);
      const payload = `symbol=BTCUSDT&recvWindow=20000&timestamp=${stamp}`;
      expect(browser.sign).toHaveBeenLastCalledWith(expect.objectContaining({ wsAuth: {
        toSign: JSON.stringify({ slot: 'primary', payload }), outputFormat: 'asterdex-hmac',
      } }));
      expect(local.config?.params?.signature).toBe(createHmac('sha256', 'fixture-material').update(payload).digest('hex'));
    }
    expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
    expect(JSON.stringify(browser.sign.mock.calls)).not.toContain('fixture-material');
    expect(JSON.stringify(browser.logs)).not.toContain('fixture-material');
  }
});

it('actual WASM wallet device matches independent ethers golden bytes for EIP-191 and Code EIP-712', async () => {
  for (const vector of corpus) {
    stamp = vector.nowMs;
    const browser = device({ secret: vector.privateKey, testnet: vector.testnet, applicationId: vector.user,
      auth: { userAddress: vector.user, signerAddress: vector.signer }, extra: { asterdexApiVersion: 'v3' } });
    const provider = createRemoteSignerProvider({ exchangeName: 'asterdex', identities: { primary }, remoteSigner: browser.remoteSigner });
    const keys = vector.name.startsWith('documented') ? ['symbol', 'type', 'side', 'timeInForce', 'quantity', 'price']
      : vector.name.startsWith('decimal') ? ['symbol', 'quantity', 'price', 'reduceOnly', 'note']
      : vector.name.startsWith('query') ? ['symbol', 'note'] : [];
    for (const method of ['GET', 'POST']) {
      const statement = JSON.stringify({ method, path: '/fapi/v3/order', query: keys.map((key) => [key, (vector.params as Record<string, string>)[key]]),
        user: vector.user, signer: vector.signer, nonce: vector.nonce, stampMs: stamp, testnet: vector.testnet });
      const digest = method === 'POST' ? vector.codeHash : vector.hash;
      const work: SignRequest = { receipt: { nonce: vector.nonce, ...(method === 'GET' ? { stampMs: stamp } : {}) },
        context: { venue: 'asterdex', method, host: '', path: '/fapi/v3/order' }, operations: [
          { kind: 'present', key: primary.credential },
          { kind: 'sha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(statement)) }] },
          { kind: 'sign', key: primary.signing, algorithm: 'secp256k1Digest', payload: [{ kind: 'bytes', bytes: Array.from(getBytes(digest)) }] },
        ] };
      const result = await provider(work, { signal: new AbortController().signal });
      const signature = new Wallet(vector.privateKey).signingKey.sign(digest);
      expect(result.values).toEqual([Array.from(new TextEncoder().encode('fixture-key')),
        Array.from(createHash('sha256').update(statement).digest()), [...getBytes(signature.serialized).slice(0, 64), signature.yParity]]);
      expect(result.receipt).toEqual(work.receipt);
      expect(JSON.stringify(browser.sign.mock.calls)).not.toContain(vector.privateKey);
      expect(JSON.stringify(browser.sign.mock.calls)).not.toContain('origin/');
      const input: RemoteSignerRequest = { id: 'negative', type: 'ws-auth', exchangeName: 'asterdex', wsAuth: {
        outputFormat: 'asterdex-wallet', toSign: JSON.stringify({ slot: 'primary', payload: statement.replace('"method":', '"unknown":true,"method":') }),
      } };
      expect((await browser.exchange.signWsAuth(input)).wsAuthSignature).toBeUndefined();
    }
    expect(browser.io).not.toHaveBeenCalled();
    expect(JSON.stringify(browser.logs)).not.toContain(vector.privateKey);
    expect(browser.exchange.options).not.toHaveProperty('secret');
  }
});

it('the device selects secondary HMAC custody, and V1 PayloadSigner signs the same REST payload', async () => {
  const browser = device({ secondary_key: 'read-key', secondary_secret: 'read-material' });
  const secondary = { credential: 'origin/read-key', signing: 'origin/read-sign', publicCredential: 'read-key' };
  const provider = createRemoteSignerProvider({ exchangeName: 'asterdex', identities: { primary, secondary }, remoteSigner: browser.remoteSigner });
  const payload = `recvWindow=20000&timestamp=${stamp}`;
  const result = await provider({ receipt: { stampMs: stamp }, context: { venue: 'asterdex', method: 'GET', host: '', path: '' }, operations: [
    { kind: 'present', key: secondary.credential }, { kind: 'sign', key: secondary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(payload)) }] },
  ] }, { signal: new AbortController().signal });
  expect(result.values[1]).toEqual(Array.from(createHmac('sha256', 'read-material').update(payload).digest()));
  expect(browser.sign).toHaveBeenLastCalledWith(expect.objectContaining({ wsAuth: { outputFormat: 'asterdex-hmac', toSign: JSON.stringify({ slot: 'secondary', payload }) } }));
  const payloadSigner = vi.fn(({ payload: text }) => ({ signature: createHmac('sha256', 'fixture-material').update(text).digest('hex') }));
  const server = device({ secret: '', payloadSigner });
  const request: RemoteSignerRequest = { id: 'payload', type: 'http', exchangeName: 'asterdex', config: { method: 'get', url: '/fapi/v4/account', baseURL: 'https://fapi.asterdex.com' } };
  expect((await server.exchange.signHttpRequest(request)).error).toBeUndefined();
  expect(payloadSigner).toHaveBeenCalledWith(expect.objectContaining({ payload, scheme: 'hmac-sha256', encoding: 'hex' }));
});

it.each(['v1', 'v3'] as const)('actual %s native startup uses keyless hosted REST and listen-key WS authentication', async (version) => {
  const { EngineHost } = await import('@tealstreet/safe-cex-wasm/host');
  const { WasmEngine } = await loadNodeEngine();
  const vector = corpus[0]; stamp = vector.nowMs;
  const wallet = version === 'v3';
  const options = { extra: { asterdexApiVersion: version }, ...(wallet ? {
    applicationId: vector.user, auth: { userAddress: vector.user, signerAddress: vector.signer },
  } : {}) };
  const browser = device({ ...options, secret: wallet ? vector.privateKey : 'fixture-material' });
  const provider = createRemoteSignerProvider({ exchangeName: 'asterdex', identities: { primary }, remoteSigner: browser.remoteSigner });
  const engine = new WasmEngine('asterdex', JSON.stringify({ remoteSigning: true, signing: { primary: { credential: primary.credential, signing: primary.signing } },
    applicationId: wallet ? vector.user : undefined, asterdex: { apiVersion: version, userAddress: wallet ? vector.user : undefined, signerAddress: wallet ? vector.signer : undefined } }));
  const packets: Array<{ method: string; url: string }> = [];
  const sockets: string[] = [];
  const host = new EngineHost(engine, { now: () => stamp, prepareHttp: prepareRecordedHttp,
    request: async ({ method, url }) => {
      packets.push({ method, url });
      const path = new URL(url).pathname;
      const data = path.endsWith('/time') ? JSON.stringify({ serverTime: stamp })
        : path.endsWith('/exchangeInfo') ? '{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000"}]}]}'
        : path.endsWith('/account') ? '{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"}'
        : path.endsWith('/positionSide/dual') ? '{"dualSidePosition":false}'
        : path.endsWith('/listenKey') ? '{"listenKey":"fixture-session"}' : '[]';
      return { status: 200, headers: '', body: new TextEncoder().encode(data) };
    }, connect: (url, handlers) => { sockets.push(url); queueMicrotask(() => handlers.onOpen()); return { send: () => {}, close: () => {} }; },
    setTimeout: () => 0, clearTimeout: () => {},
  }, {}, { ownsEngine: true, entropy: (count) => new Uint8Array(count).fill(7), signingEntropy: (count) => new Uint8Array(count).fill(7),
    signer: provider, signerAlgorithms: ['hmacSha256', 'secp256k1Digest'] });
  try {
    await host.prepareAddressSetup(); host.start();
    await vi.waitFor(() => expect(sockets.some((url) => url.endsWith('/ws/fixture-session'))).toBe(true));
    expect(packets.filter(({ url }) => url.includes('/listenKey'))).toHaveLength(1);
    expect(packets.filter(({ url }) => url.includes('signature=')).length).toBeGreaterThan(4);
    expect(browser.sign).toHaveBeenCalled();
    expect(JSON.stringify(browser.sign.mock.calls)).not.toContain(wallet ? vector.privateKey : 'fixture-material');
    expect(browser.io).not.toHaveBeenCalled();
  } finally { host.dispose(); }
});
