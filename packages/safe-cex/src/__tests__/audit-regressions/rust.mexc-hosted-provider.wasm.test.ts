import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';
import { OrderSide, OrderType } from '../../types';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { request, vectors } from '../support/mexcHostedSigning';

const stamp = 1_700_000_000_000;
const material = 'replay-secret';
const mac = (payload: string) => createHmac('sha256', material).update(payload).digest('hex');
const live: RustEngineExchange[] = [];
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport(); vi.restoreAllMocks();
});
const fixture = (options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('Signing capture must not use network'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange('mexc', { engine: 'rust', key: 'replay-key', secret: material,
    applicationId: 'fixture-app', ...options }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
};
const signer = (browser: RustEngineExchange) => {
  const sign = vi.fn((input: RemoteSignerRequest) => browser.signWsAuth(input));
  const remoteSigner: RemoteSigner = { sign, accountName: 'fixture-account', isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return { sign, remoteSigner };
};

it.each([
  ['/api/v1/private/order/submit', '{"symbol":"BTC_USDT","vol":1,"price":60000}'],
  ['/api/v1/private/order/submit_batch', '[{"symbol":"BTC_USDT","vol":1,"price":60000}]'],
  ['/api/v1/private/planorder/place/v2', '{"symbol":"BTC_USDT","triggerPrice":55000,"vol":1}'],
  ['/api/v1/private/stoporder/place', '{"positionId":"fixture-position","stopLossPrice":55000}'],
])('actual WASM broker attribution on %s survives every hosted capture provider', async (path, body) => {
  const browser = fixture();
  const { remoteSigner } = signer(browser.exchange);
  const remote = fixture({ secret: '', remoteSigner });
  const payload = fixture({ secret: '', payloadSigner: ({ payload }) => ({ signature: mac(payload) }) });
  for (const target of [browser, remote, payload]) {
    const result = await target.exchange.signHttpRequest(request({ ...vectors[2], method: 'POST', params: [], path, body }));
    expect(result.error).toBeUndefined();
    expect(result.config?.headers.source).toBe('TEAL');
    expect(result.config?.headers.Signature).toBe(mac(`replay-key${stamp}${body}`));
    expect(result.config?.data).toBe(body);
    expect(target.io).not.toHaveBeenCalled();
  }
});

it.each(vectors)('actual WASM MEXC $label: keyless RemoteSigner matches local payload and signature bytes', async (vector) => {
  const browser = fixture();
  const { sign, remoteSigner } = signer(browser.exchange);
  const fallback = vi.fn(() => { throw new Error('No PayloadSigner fallback'); });
  const server = fixture({ secret: '', remoteSigner, payloadSigner: fallback });
  const local = await browser.exchange.signHttpRequest(request(vector));
  const delegated = await server.exchange.signHttpRequest(request(vector));
  expect(local.error).toBeUndefined(); expect(delegated.error).toBeUndefined();
  expect(delegated.config).toEqual(local.config);
  expect(delegated.config?.headers['Signature']).toBe(vector.signature);
  expect(delegated.config?.headers.source).toBe('TEAL');
  expect(sign).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'mexc',
    accountName: 'fixture-account', wsAuth: { toSign: vector.payload, outputFormat: 'hex' } }));
  expect(server.exchange.options).not.toHaveProperty('secret');
  expect(server.exchange.options).not.toHaveProperty('remoteSigner');
  expect(fallback).not.toHaveBeenCalled();
  expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
});

it('actual WASM MEXC WS signing matches the local key and the native auth recipe', async () => {
  const browser = fixture(); const { sign, remoteSigner } = signer(browser.exchange);
  const server = fixture({ secret: '', remoteSigner });
  const input: RemoteSignerRequest = { id: 'auth', type: 'ws-auth', exchangeName: 'mexc', wsAuth: { toSign: `replay-key${stamp}`, outputFormat: 'hex' } };
  const local = await browser.exchange.signWsAuth(input);
  expect(await server.exchange.signWsAuth(input)).toEqual(local);
  expect(local).toEqual({ id: 'auth', wsAuthSignature: mac(`replay-key${stamp}`) });
  expect(sign).toHaveBeenCalledOnce();
});

it.each([' \n { "vol": 1 } \t ', '\n["739113577038255616"]\r\n'])('actual WASM authenticates exact replay body bytes %j', async (body) => {
  const browser = fixture(); const { remoteSigner } = signer(browser.exchange);
  const server = fixture({ secret: '', remoteSigner });
  const input = request(vectors[2]); input.config!.data = body;
  const result = await server.exchange.signHttpRequest(input);
  expect(result.error).toBeUndefined();
  expect(result.config?.data).toBe(body);
  expect(result.config?.headers.Signature).toBe(mac(`replay-key${stamp}${result.config?.data}`));
  expect(server.io).not.toHaveBeenCalled();
});

it('provider failures and malformed results cannot leak material, send I/O or fall back', async () => {
  const logs = [vi.spyOn(console, 'error').mockImplementation(() => {}), vi.spyOn(console, 'warn').mockImplementation(() => {}), vi.spyOn(console, 'log').mockImplementation(() => {})];
  for (const mode of ['error', 'malformed', 'unavailable'] as const) {
    const sign = vi.fn(async (input: RemoteSignerRequest) => mode === 'error'
      ? { id: input.id, error: `fixture provider error ${material}` } : { id: input.id, wsAuthSignature: '00'.repeat(31) });
    const fallback = vi.fn(() => { throw new Error('No fallback'); });
    const remoteSigner: RemoteSigner = { sign, isAvailable: () => mode !== 'unavailable', onAvailabilityChange: () => () => {} };
    const server = fixture({ remoteSigner, payloadSigner: fallback });
    const result = await server.exchange.signHttpRequest(request(vectors[2]));
    expect(result.error).toBeTruthy(); expect(result.config).toBeUndefined();
    expect(JSON.stringify(result)).not.toContain(material);
    expect(server.io).not.toHaveBeenCalled(); expect(fallback).not.toHaveBeenCalled();
  }
  expect(JSON.stringify(logs.map((log) => log.mock.calls))).not.toContain(material);
});

it('actual WASM MEXC PayloadSigner signs canonical REST bytes without local material', async () => {
  const payloadSigner = vi.fn(({ payload }: { payload: string }) => ({ signature: mac(payload) }));
  const server = fixture({ secret: '', payloadSigner });
  const result = await server.exchange.signHttpRequest(request(vectors[2]));
  expect(result.error).toBeUndefined();
  expect(result.config?.headers.Signature).toBe(vectors[2].signature);
  expect(payloadSigner).toHaveBeenCalledExactlyOnceWith({ exchangeName: 'mexc', key: 'replay-key',
    payload: vectors[2].payload, scheme: 'hmac-sha256', encoding: 'hex' });
  expect(server.io).not.toHaveBeenCalled();
});

it('actual WASM MEXC saved secondary keys follow the SDK bypass policy', async () => {
  const browser = fixture(); const { sign, remoteSigner } = signer(browser.exchange);
  const server = fixture({ secret: '', secondary_key: 'read-key', secondary_secret: 'fixture-read', remoteSigner });
  const result = await server.exchange.signHttpRequest(request(vectors[0]));
  expect(result.config?.headers.ApiKey).toBe('replay-key');
  expect(result.config?.headers.Signature).toBe(vectors[0].signature);
  expect(sign).toHaveBeenCalledOnce();
  expect(server.exchange.options).not.toHaveProperty('secondary_key');
});

const markets = {"success":true,"code":0,"data":[{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true,"contractSize":0.0001,"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500,"makerFeeRate":0,"takerFeeRate":0.0002}]};

const tickers = {"success":true,"code":0,"data":[{"symbol":"BTC_USDT","lastPrice":60000,"fairPrice":60001,"indexPrice":59999,"bid1":59999.9,"ask1":60000,"riseFallRate":0.01,"holdVol":10000,"fundingRate":0.0001,"volume24":50000,"amount24":300000}]};

it.each(['remote', 'payload', 'payload-headers', 'remote-trimmed'] as const)('actual WASM MEXC %s startup signs REST, retains identity and selects the private feed policy', async (mode) => {
  const browser = fixture(); const { sign, remoteSigner } = signer(browser.exchange);
  const payloadSigner = vi.fn(({ payload }: { payload: string }) => ({ signature: mac(payload), ...(mode === 'payload-headers' ? { headers: { 'X-Trace': 'fixture-trace', apikey: 'spoofed', signature: 'spoofed', 'request-time': '1', 'recv-window': '1', 'content-type': 'spoofed', source: 'spoofed' } } : {}) }));
  const logins: string[] = [];
  const requests: Array<{ url: string; method: string; headers: Record<string, string>; body?: string }> = [];
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp,
    request: async ({ url, method, headers, body }) => {
      requests.push({ url, method, headers, body: typeof body === 'string' ? body : undefined });
      const data = url.includes('/contract/detail') ? markets : url.includes('/contract/ticker') ? tickers
        : { success: true, code: 0, data: url.includes('/order/submit') ? 'fixture-order' : url.includes('/position_mode') ? 2 : [] };
      return { status: 200, body: new TextEncoder().encode(JSON.stringify(data)) };
    },
    connect: (_url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return { close() {}, send(text) {
        if (!text.startsWith('{') || JSON.parse(text).method !== 'login') return;
        logins.push(text);
        queueMicrotask(() => handlers.onMessage('{"channel":"rs.login","success":true,"data":"success"}'));
      } };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const server = new RustEngineExchange('mexc', { engine: 'rust', key: 'replay-key', secret: '',
    applicationId: 'fixture-app', ...(mode.startsWith('payload') ? { payloadSigner } : { remoteSigner }),
    ...(mode === 'remote-trimmed' ? { key: ' replay-key ' } : {}) }, new DefaultStore()); live.push(server);
  const errors: unknown[] = [];
  void server.start().catch((error) => errors.push(error));
  await vi.waitFor(() => expect(requests.some((r) => r.url.includes('/private/account/assets'))).toBe(true));
  expect(await server.getAccount()).toMatchObject({ userId: mode === 'remote-trimmed' ? 'mexc:ae0c5396072d0d2a609af843' : 'mexc:a7c5c5f5cbc33ea500a4850d' });
  await vi.waitFor(() => expect(server.store.loaded.orders).toBe(true));
  expect(await server.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy,
    amount: 60, price: 60_000 })).toEqual(['fixture-order']);
  expect(requests.filter((r) => r.method === 'POST' && r.url.endsWith('/order/submit'))).toHaveLength(1);
  const privateRequests = requests.filter((r) => new URL(r.url).pathname.startsWith('/api/v1/private/'));
  expect(privateRequests.length).toBeGreaterThan(0);
  for (const r of privateRequests) {
    const headers = Object.fromEntries(Object.entries(r.headers).map(([key, value]) => [key.toLowerCase(), value]));
    const url = new URL(r.url);
    expect(headers.signature).toBe(mac(`replay-key${stamp}${r.method === 'GET' ? url.search.slice(1) : r.body ?? ''}`));
    expect(headers.apikey).toBe('replay-key');
    expect(headers.source).toBe('TEAL');
    if (mode === 'payload-headers') expect(r.headers['X-Trace']).toBe('fixture-trace');
    expect(r.headers['Content-Type']).toBe('application/json');
    expect(r.headers['Recv-Window']).toBe('20000');
    expect(headers['request-time']).toBe(String(stamp));
  }
  if (!mode.startsWith('payload')) {
    await vi.waitFor(() => expect(logins).toHaveLength(1));
    expect(JSON.parse(logins[0])).toEqual({ method: 'login', param: { apiKey: 'replay-key', reqTime: String(stamp), signature: mac(`replay-key${stamp}`) } });
    expect(sign.mock.calls.some(([input]) => input.wsAuth?.toSign === `replay-key${stamp}`)).toBe(true);
  } else {
    expect(logins).toEqual([]);
    expect(payloadSigner).toHaveBeenCalled();
  }
  expect(errors).toEqual([]);
});
