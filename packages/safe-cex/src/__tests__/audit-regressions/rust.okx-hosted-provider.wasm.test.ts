import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

const stamp = 1_709_209_849_123;
const iso = '2024-02-29T12:30:49.123Z';
const aliases = ['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const;
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
  setRustEngineTransport(() => ({ now: () => stamp, request: io, connect: io,
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
  config: { method: 'POST', url: '/api/v5/trade/order', baseURL: 'https://fixture.invalid',
    params: { instId: 'BTC-USDT-SWAP', note: 'é/+%' }, data: { sz: '0.0100', side: 'buy' },
    headers: { 'X-Transport': 'keep', 'OK-ACCESS-KEY': 'stale', 'OK-ACCESS-PASSPHRASE': 'stale' } } });
const payload = `${iso}POST/api/v5/trade/order?instId=BTC-USDT-SWAP&note=é/+%{"sz":"0.0100","side":"buy"}`;

it.each(aliases)('actual WASM %s signs hosted HTTP and both WS preimages with primary custody only', async (name) => {
  const f = fixture(name, { secondary_key: 'secondary-public', secondary_secret: 'fixture-secondary-material', secondary_passphrase: 'fixture-secondary-passphrase' });
  const request = http(name); const original = structuredClone(request);
  const result = await f.exchange.signHttpRequest(request);
  expect(result.error).toBeUndefined();
  expect(result.config).toMatchObject({ url: request.config!.url, params: request.config!.params,
    data: '{"sz":"0.0100","side":"buy"}', headers: {
      'OK-ACCESS-KEY': 'owned-primary-public', 'OK-ACCESS-SIGN': mac(payload),
      'OK-ACCESS-PASSPHRASE': 'fixture-primary-passphrase', 'OK-ACCESS-TIMESTAMP': iso, 'X-Transport': 'keep',
      ...(name.endsWith('demo') ? { 'x-simulated-trading': '1' } : {}),
    } });
  for (const channel of ['private', 'business']) {
    const toSign = '1709209800GET/users/self/verify';
    await expect(f.exchange.signWsAuth({ id: channel, type: 'ws-auth', exchangeName: name,
      wsAuth: { toSign, outputFormat: 'base64', extraData: { timestamp: 1709209800 } } }))
      .resolves.toEqual({ id: channel, wsAuthSignature: mac(toSign) });
  }
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
  expect(result.error).toBeUndefined(); expect(result.config?.headers['OK-ACCESS-SIGN']).toBe(mac(payload));
  expect(result.config?.headers['OK-ACCESS-PASSPHRASE']).toBe('fixture-primary-passphrase');
  expect(sign).toHaveBeenNthCalledWith(1, expect.objectContaining({ type: 'ws-auth', exchangeName: name, accountName: 'owned-account',
    wsAuth: { toSign: payload, outputFormat: 'base64' } }));
  for (const channel of ['private', 'business']) {
    const toSign = '1709209800GET/users/self/verify';
    const result = await server.exchange.signWsAuth({ id: channel, type: 'ws-auth', exchangeName: name,
      wsAuth: { toSign, outputFormat: 'base64', extraData: { timestamp: 1709209800 } } });
    expect(result).toEqual({ id: channel, wsAuthSignature: mac(toSign) });
    expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: name, accountName: 'owned-account',
      wsAuth: { toSign, outputFormat: 'base64', extraData: { timestamp: 1709209800 } } }));
  }
  expect(sign).toHaveBeenCalledTimes(3); expect(fallback).not.toHaveBeenCalled();
  expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
});

it.each(aliases)('actual WASM %s private PayloadSigner refuses synchronously as typed unsupported', (name) => {
  let error: unknown;
  try { fixture(name, { payloadSigner: () => ({ signature: Buffer.alloc(32).toString('base64') }) }); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
});

it.each(aliases)('actual WASM %s native private/business login dispatch uses the keyless browser provider', async (name) => {
  const browser = fixture(name);
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'native-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const logins: Array<{ channel: string; text: string }> = [];
  const requests: Array<{ url: string; headers: Record<string, string> }> = [];
  const market = { instId: 'BTC-USDT-SWAP', instType: 'SWAP', ctType: 'linear', ctVal: '0.01', ctValCcy: 'BTC', settleCcy: 'USDT',
    state: 'live', lotSz: '0.01', tickSz: '0.1', minSz: '0.01', maxLmtSz: '1000', maxMktSz: '1000', lever: '100' };
  setRustEngineTransport(() => ({
    prepareHttp: prepareRecordedHttp,
    now: () => stamp,
    request: async ({ url, headers }) => {
      requests.push({ url, headers });
      let data: unknown = [];
      if (url.includes('/instruments')) data = [url.includes('instType=FUTURES')
        ? { ...market, instId: 'BTC-USDT-261225', instType: 'FUTURES', ruleType: 'xperp' } : market];
      else if (url.includes('/account/config')) data = [{ posMode: 'net_mode', acctLv: '2' }];
      else if (url.includes('/account-position-risk')) data = [{ balData: [{ ccy: 'USDT', eq: '100' }] }];
      else if (url.includes('/tickers')) data = [{ instId: market.instId, last: '50000', bidPx: '49999', askPx: '50001', vol24h: '1' }];
      return { status: 200, body: new TextEncoder().encode(JSON.stringify({ code: '0', data })) };
    },
    connect: (url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return { close() {}, send(text) {
        if (!text.startsWith('{')) return;
        const payload = JSON.parse(text);
        if (payload.op !== 'login') return;
        const channel = url.endsWith('/business') ? 'business' : 'private';
        logins.push({ channel, text });
        queueMicrotask(() => handlers.onMessage('{"event":"login","code":"0"}'));
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
    expect(logins.map((login) => login.channel).sort()).toEqual(['business', 'private']);
    expect(requests.some((request) => request.url.includes('/account/config'))).toBe(true);
  });
  const seconds = 1_709_209_849;
  const preimage = `${seconds}GET/users/self/verify`;
  for (const login of logins) {
    expect(JSON.parse(login.text)).toEqual({ op: 'login', args: [{ apiKey: 'owned-primary-public',
      passphrase: 'fixture-primary-passphrase', timestamp: String(seconds), sign: mac(preimage) }] });
  }
  const wsRequests = sign.mock.calls.map(([request]) => request).filter((request) => request.wsAuth?.toSign === preimage);
  expect(wsRequests).toHaveLength(2);
  for (const request of wsRequests) expect(request).toMatchObject({ type: 'ws-auth', exchangeName: name, accountName: 'native-account',
    wsAuth: { toSign: preimage, outputFormat: 'base64', extraData: { timestamp: seconds } } });
  for (const request of requests.filter((request) => /\/api\/v5\/(account|trade)\//.test(request.url))) {
    expect(request.headers).toMatchObject({ 'OK-ACCESS-KEY': 'owned-primary-public',
      'OK-ACCESS-TIMESTAMP': iso, 'OK-ACCESS-PASSPHRASE': 'fixture-primary-passphrase' });
    expect(request.headers['OK-ACCESS-SIGN']).toMatch(/^[A-Za-z0-9+/]{43}=$/);
  }
  for (const request of requests.filter((request) => request.headers['OK-ACCESS-SIGN'])) {
    const url = new URL(request.url);
    expect(request.headers['OK-ACCESS-SIGN']).toBe(mac(`${iso}GET${url.pathname}${url.search}`));
    expect(request.headers['OK-ACCESS-PASSPHRASE']).toBe('fixture-primary-passphrase');
  }
  expect(requests.some((request) => request.headers['OK-ACCESS-SIGN'])).toBe(true);
  expect(startupErrors).toEqual([]);
});
