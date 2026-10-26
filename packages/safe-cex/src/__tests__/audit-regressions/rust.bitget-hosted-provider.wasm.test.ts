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
const seconds = 1_709_209_849;
const aliases = ['bitget', 'bitgetusdc', 'bitgetinverse'] as const;
type Alias = typeof aliases[number];
const live: RustEngineExchange[] = [];
const mac = (payload: string) => createHmac('sha256', 'fixture-primary-material').update(payload).digest('base64');
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport();
});

const fixture = (name: Alias, options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('Signing capture must not use network'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const store = new DefaultStore();
  const input: ExchangeOptions = { engine: 'rust', key: 'owned-primary-public', secret: 'fixture-primary-material',
    passphrase: 'fixture-primary-passphrase', ...options };
  const before = { ...input };
  const exchange = new RustEngineExchange(name, input, store);
  live.push(exchange);
  return { exchange, store, input, before, io };
};
const http = (exchangeName: Alias): RemoteSignerRequest => ({ id: 'hosted-http', type: 'http', exchangeName,
  config: { method: 'POST', url: '/api/v2/mix/order/place-order', baseURL: 'https://fixture.invalid',
    params: { symbol: 'BTCUSDT', note: 'é/+%' }, data: { symbol: 'BTCUSDT', size: '0.0100', side: 'buy' },
    headers: { 'X-Transport': 'keep', 'ACCESS-KEY': 'stale', 'ACCESS-PASSPHRASE': 'stale' } } });
const payload = `${stamp}POST/api/v2/mix/order/place-order?symbol=BTCUSDT&note=é/+%{"symbol":"BTCUSDT","size":"0.0100","side":"buy"}`;
const login = `${seconds}GET/user/verify`;

it.each(aliases)('actual WASM %s signs hosted HTTP and the private WS preimage with primary custody only', async (name) => {
  const f = fixture(name, { secondary_key: 'secondary-public', secondary_secret: 'fixture-secondary-material', secondary_passphrase: 'fixture-secondary-passphrase' });
  const request = http(name); const original = structuredClone(request);
  const result = await f.exchange.signHttpRequest(request);
  expect(result.error).toBeUndefined();
  expect(result.config).toMatchObject({ url: request.config!.url, params: request.config!.params,
    data: '{"symbol":"BTCUSDT","size":"0.0100","side":"buy"}', headers: {
      'ACCESS-KEY': 'owned-primary-public', 'ACCESS-SIGN': mac(payload), 'ACCESS-TIMESTAMP': String(stamp),
      'ACCESS-PASSPHRASE': 'fixture-primary-passphrase', 'X-CHANNEL-API-CODE': 'Tealstreet', 'X-Transport': 'keep',
    } });
  await expect(f.exchange.signWsAuth({ id: 'private', type: 'ws-auth', exchangeName: 'bitget',
    wsAuth: { toSign: login, outputFormat: 'base64', extraData: { timestamp: seconds } } }))
    .resolves.toEqual({ id: 'private', wsAuthSignature: mac(login) });
  expect(request).toEqual(original); expect(f.input).toEqual(f.before);
  expect(f.io).not.toHaveBeenCalled(); expect(f.exchange.hasStarted).toBe(false);
  expect(Object.values(f.store.loaded).some(Boolean)).toBe(false);
  for (const key of ['key', 'secret', 'passphrase', 'secondary_key', 'secondary_secret', 'secondary_passphrase'])
    expect(f.exchange.options).not.toHaveProperty(key);
});

it.each(aliases)('actual WASM %s keyless RemoteSigner reaches the browser provider on unchanged HTTP/WS wire', async (name) => {
  const browser = fixture(name);
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'owned-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const fallback = vi.fn(() => { throw new Error('No local or PayloadSigner fallback'); });
  const server = fixture(name, { secret: '', remoteSigner, payloadSigner: fallback });
  const result = await server.exchange.signHttpRequest(http(name));
  expect(result.error).toBeUndefined(); expect(result.config?.headers['ACCESS-SIGN']).toBe(mac(payload));
  expect(result.config?.headers['ACCESS-PASSPHRASE']).toBe('fixture-primary-passphrase');
  expect(sign).toHaveBeenNthCalledWith(1, expect.objectContaining({ type: 'ws-auth', exchangeName: name, accountName: 'owned-account',
    wsAuth: { toSign: payload, outputFormat: 'base64' } }));
  const ws = await server.exchange.signWsAuth({ id: 'private', type: 'ws-auth', exchangeName: 'bitget',
    wsAuth: { toSign: login, outputFormat: 'base64', extraData: { timestamp: seconds } } });
  expect(ws).toEqual({ id: 'private', wsAuthSignature: mac(login) });
  expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'bitget', accountName: 'owned-account',
    wsAuth: { toSign: login, outputFormat: 'base64', extraData: { timestamp: seconds } } }));
  expect(sign).toHaveBeenCalledTimes(2); expect(fallback).not.toHaveBeenCalled();
  expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
});

it.each(aliases)('actual WASM %s private PayloadSigner refuses synchronously as typed unsupported', (name) => {
  let error: unknown;
  try { fixture(name, { payloadSigner: () => ({ signature: Buffer.alloc(32).toString('base64') }) }); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
});

it.each(aliases)('actual WASM %s native private login and signed HTTP dispatch use the keyless browser provider', async (name) => {
  const browser = fixture(name);
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'native-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const logins: string[] = [];
  const requests: Array<{ url: string; headers: Record<string, string> }> = [];
  const quote = name === 'bitgetinverse' ? 'USD' : name === 'bitgetusdc' ? 'USDC' : 'USDT';
  const symbol = name === 'bitgetusdc' ? 'BTCPERP' : `BTC${quote}`;
  setRustEngineTransport(() => ({
    prepareHttp: prepareRecordedHttp,
    now: () => stamp,
    request: async ({ url, headers }) => {
      requests.push({ url, headers });
      let body: unknown = { code: '00000', data: [] };
      if (url.includes('/contracts')) body = { code: '00000', data: [{ symbol, baseCoin: 'BTC', quoteCoin: quote, symbolStatus: 'normal',
        pricePlace: '1', priceEndStep: '1', sizeMultiplier: '0.001', minTradeNum: '0.001', maxOrderQty: '120', maxLever: '125' }] };
      else if (url.includes('/account/settings')) body = { code: '40084', msg: 'Classic Account mode', data: null };
      return { status: 200, body: new TextEncoder().encode(JSON.stringify(body)) };
    },
    connect: (url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return { close() {}, send(text) {
        if (!text.startsWith('{') || JSON.parse(text).op !== 'login' || !url.endsWith('/private')) return;
        logins.push(text);
        queueMicrotask(() => handlers.onMessage('{"event":"login","code":0}'));
      } };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const server = new RustEngineExchange(name, { engine: 'rust', key: 'owned-primary-public', secret: '',
    passphrase: 'fixture-primary-passphrase', remoteSigner }, new DefaultStore());
  live.push(server);
  const startupErrors: unknown[] = [];
  void server.start().catch((error) => startupErrors.push(error));
  await vi.waitFor(() => {
    expect(logins).toHaveLength(1);
    expect(requests.some((request) => request.headers['ACCESS-SIGN'])).toBe(true);
  });
  expect(JSON.parse(logins[0])).toEqual({ op: 'login', args: [{ apiKey: 'owned-primary-public',
    passphrase: 'fixture-primary-passphrase', timestamp: String(seconds), sign: mac(login) }] });
  const wsRequests = sign.mock.calls.map(([request]) => request).filter((request) => request.wsAuth?.toSign === login);
  expect(wsRequests).toHaveLength(1);
  expect(wsRequests[0]).toMatchObject({ type: 'ws-auth', exchangeName: 'bitget', accountName: 'native-account',
    wsAuth: { toSign: login, outputFormat: 'base64', extraData: { timestamp: seconds } } });
  for (const request of requests.filter((request) => request.headers['ACCESS-SIGN'])) {
    const url = new URL(request.url);
    expect(request.headers).toMatchObject({ 'ACCESS-KEY': 'owned-primary-public', 'ACCESS-PASSPHRASE': 'fixture-primary-passphrase',
      'ACCESS-TIMESTAMP': String(stamp), 'X-CHANNEL-API-CODE': 'Tealstreet' });
    expect(request.headers['ACCESS-SIGN']).toBe(mac(`${stamp}GET${url.pathname}${url.search}`));
  }
  expect(startupErrors).toEqual([]);
});
