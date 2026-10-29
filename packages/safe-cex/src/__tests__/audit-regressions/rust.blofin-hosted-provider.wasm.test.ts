vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

const stamp = 1_709_209_849_123;
const live: RustEngineExchange[] = [];
// BloFin signs base64 of the HMAC's lowercase hex text (independent Node oracle).
const mac = (payload: string) =>
  Buffer.from(createHmac('sha256', 'fixture-primary-material').update(payload).digest('hex')).toString('base64');
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport();
});

const fixture = (options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('Signing capture must not use network'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const store = new DefaultStore();
  const input: ExchangeOptions = { engine: 'rust', key: 'owned-primary-public', secret: 'fixture-primary-material',
    passphrase: 'fixture-primary-passphrase', ...options };
  const before = { ...input };
  const exchange = new RustEngineExchange('blofin', input, store);
  live.push(exchange);
  return { exchange, store, input, before, io };
};
const http = (): RemoteSignerRequest => ({ id: 'hosted-http', type: 'http', exchangeName: 'blofin',
  config: { method: 'POST', url: '/api/v1/trade/order', baseURL: 'https://fixture.invalid',
    params: { note: 'é/+%' }, data: { instId: 'BTC-USDT', side: 'buy', size: '0.1' },
    headers: { 'X-Transport': 'keep', 'ACCESS-KEY': 'stale', 'ACCESS-PASSPHRASE': 'stale' } } });
const payload = (nonce: string) =>
  `/api/v1/trade/order?note=é/+%POST${stamp}${nonce}{"instId":"BTC-USDT","side":"buy","size":"0.1"}`;
const login = (nonce: string) => `/users/self/verifyGET${stamp}${nonce}`;

it('actual WASM BloFin signs hosted HTTP and the private WS preimage with primary custody only', async () => {
  const f = fixture({ secondary_key: 'secondary-public', secondary_secret: 'fixture-secondary-material', secondary_passphrase: 'fixture-secondary-passphrase' });
  const request = http(); const original = structuredClone(request);
  const result = await f.exchange.signHttpRequest(request);
  expect(result.error).toBeUndefined();
  const nonce = result.config?.headers['ACCESS-NONCE'] as string;
  expect(result.config).toMatchObject({ url: request.config!.url, params: request.config!.params,
    data: '{"instId":"BTC-USDT","side":"buy","size":"0.1"}', headers: {
      'ACCESS-KEY': 'owned-primary-public', 'ACCESS-SIGN': mac(payload(nonce)), 'ACCESS-TIMESTAMP': String(stamp),
      'ACCESS-PASSPHRASE': 'fixture-primary-passphrase', 'X-Transport': 'keep',
    } });
  await expect(f.exchange.signWsAuth({ id: 'private', type: 'ws-auth', exchangeName: 'blofin',
    wsAuth: { toSign: login('n1'), outputFormat: 'hex-base64' } }))
    .resolves.toEqual({ id: 'private', wsAuthSignature: mac(login('n1')) });
  expect(request).toEqual(original); expect(f.input).toEqual(f.before);
  expect(f.io).not.toHaveBeenCalled(); expect(f.exchange.hasStarted).toBe(false);
  expect(Object.values(f.store.loaded).some(Boolean)).toBe(false);
  for (const key of ['key', 'secret', 'passphrase', 'secondary_key', 'secondary_secret', 'secondary_passphrase'])
    expect(f.exchange.options).not.toHaveProperty(key);
});

it('actual WASM BloFin keyless RemoteSigner reaches the browser provider on the hex-base64 wire', async () => {
  const browser = fixture();
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'owned-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const fallback = vi.fn(() => { throw new Error('No local or PayloadSigner fallback'); });
  const server = fixture({ secret: '', remoteSigner, payloadSigner: fallback });
  const result = await server.exchange.signHttpRequest(http());
  expect(result.error).toBeUndefined();
  const nonce = result.config?.headers['ACCESS-NONCE'] as string;
  expect(result.config?.headers['ACCESS-SIGN']).toBe(mac(payload(nonce)));
  expect(result.config?.headers['ACCESS-PASSPHRASE']).toBe('fixture-primary-passphrase');
  expect(sign).toHaveBeenNthCalledWith(1, expect.objectContaining({ type: 'ws-auth', exchangeName: 'blofin', accountName: 'owned-account',
    wsAuth: { toSign: payload(nonce), outputFormat: 'hex-base64' } }));
  const ws = await server.exchange.signWsAuth({ id: 'private', type: 'ws-auth', exchangeName: 'blofin',
    wsAuth: { toSign: login('n2'), outputFormat: 'hex-base64' } });
  expect(ws).toEqual({ id: 'private', wsAuthSignature: mac(login('n2')) });
  expect(sign).toHaveBeenCalledTimes(2); expect(fallback).not.toHaveBeenCalled();
  expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
});

it('actual WASM BloFin private PayloadSigner refuses synchronously as typed unsupported', () => {
  let error: unknown;
  try { fixture({ payloadSigner: () => ({ signature: Buffer.alloc(32).toString('hex') }) }); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
});

it('actual WASM BloFin native private login and signed HTTP dispatch use the keyless browser provider', async () => {
  const browser = fixture();
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'native-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const logins: string[] = [];
  const requests: Array<{ url: string; headers: Record<string, string>; body?: string }> = [];
  setRustEngineTransport(() => ({
    prepareHttp: prepareRecordedHttp,
    now: () => stamp,
    request: async ({ url, headers, body }) => {
      requests.push({ url, headers, body: typeof body === 'string' ? body : undefined });
      let data: unknown = [];
      if (url.includes('/instruments')) data = [{ instId: 'BTC-USDT', instType: 'SWAP', contractType: 'linear', baseCurrency: 'BTC',
        quoteCurrency: 'USDT', state: 'live', contractValue: '0.001', minSize: '0.1', lotSize: '0.1', tickSize: '0.1',
        maxLimitSize: '1000', maxMarketSize: '500', maxLeverage: '150' }];
      else if (url.includes('/tickers')) data = [{ instId: 'BTC-USDT', last: '60000', open24h: '59000', bidPrice: '59999',
        askPrice: '60001', volCurrency24h: '1', vol24h: '60000' }];
      return { status: 200, body: new TextEncoder().encode(JSON.stringify({ code: '0', data })) };
    },
    connect: (url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return { close() {}, send(text) {
        if (!text.startsWith('{') || JSON.parse(text).op !== 'login') return;
        logins.push(text);
        queueMicrotask(() => handlers.onMessage('{"event":"login","code":"0","msg":""}'));
      } };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const server = new RustEngineExchange('blofin', { engine: 'rust', key: 'owned-primary-public', secret: '',
    passphrase: 'fixture-primary-passphrase', remoteSigner }, new DefaultStore());
  live.push(server);
  const startupErrors: unknown[] = [];
  void server.start().catch((error) => startupErrors.push(error));
  await vi.waitFor(() => {
    expect(logins).toHaveLength(1);
    expect(requests.some((request) => request.headers['ACCESS-SIGN'])).toBe(true);
  });
  const frame = JSON.parse(logins[0]);
  const nonce = frame.args[0].nonce;
  expect(frame).toEqual({ op: 'login', args: [{ apiKey: 'owned-primary-public', passphrase: 'fixture-primary-passphrase',
    timestamp: stamp, sign: mac(login(nonce)), nonce }] });
  expect(sign.mock.calls.filter(([request]) => request.wsAuth?.toSign === login(nonce))).toHaveLength(1);
  for (const request of requests.filter((request) => request.headers['ACCESS-SIGN'])) {
    const url = new URL(request.url);
    const method = request.body === undefined ? 'GET' : 'POST';
    expect(request.headers).toMatchObject({ 'ACCESS-KEY': 'owned-primary-public', 'ACCESS-PASSPHRASE': 'fixture-primary-passphrase',
      'ACCESS-TIMESTAMP': String(stamp) });
    expect(request.headers['ACCESS-SIGN']).toBe(
      mac(`${url.pathname}${url.search}${method}${stamp}${request.headers['ACCESS-NONCE']}${request.body ?? ''}`));
  }
  expect(startupErrors).toEqual([]);
});
