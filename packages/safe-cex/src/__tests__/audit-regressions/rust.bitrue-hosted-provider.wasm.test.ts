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
import vectors from '../../../../../sdk-rs/venues/bitrue/tests/fixtures/hosted-signing.json';

let stamp = 1_700_000_000_123;
const key = 'fixture-bitrue-public';
const secret = 'fixture-bitrue-signing-material';
const mac = (payload: string) => createHmac('sha256', secret).update(payload).digest('hex');
const timers = {
  setTimeout: (fn: () => void, ms: number) => {
    const timer = setTimeout(fn, ms);
    return () => clearTimeout(timer);
  },
  clearTimeout: (cancel: unknown) => {
    if (typeof cancel !== 'function') throw new Error('Expected fixture timer cancellation');
    cancel();
  },
};
const live: RustEngineExchange[] = [];
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
  stamp = 1_700_000_000_123;
});
const fixture = (options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('No network in signing capture'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp, request: io, connect: io,
    ...timers }));
  const exchange = new RustEngineExchange('bitrue', { key, secret, ...options }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
};
const delegate = (browser: RustEngineExchange) => {
  const sign = vi.fn((input: RemoteSignerRequest) => browser.signWsAuth(input));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return { sign, remoteSigner };
};
const request = (vector: typeof vectors[number]): RemoteSignerRequest => ({ id: 'fixture', type: 'http', exchangeName: 'bitrue',
  config: { method: vector.method, baseURL: 'https://fapi.bitrue.com', url: vector.url, ...(vector.body === null ? {} : { data: vector.body }), headers: {} } });

it.each(vectors)('actual WASM Bitrue $label: keyless server matches local device wire at fixed and corrected clocks', async (vector) => {
  for (const offset of [0, 10_000]) {
    stamp = 1_700_000_000_123 + offset;
    const browser = fixture();
    const { sign, remoteSigner } = delegate(browser.exchange);
    const fallback = vi.fn();
    const server = fixture({ secret: '', remoteSigner, payloadSigner: fallback });
    const local = await browser.exchange.signHttpRequest(request(vector));
    const delegated = await server.exchange.signHttpRequest(request(vector));
    expect(local.error).toBeUndefined();
    expect(delegated.error).toBeUndefined();
    expect(delegated.config).toEqual(local.config);
    expect(delegated.config?.url).toBe(vector.url);
    const payload = String(stamp) + vector.payload.slice(13);
    expect(delegated.config?.headers['X-CH-SIGN']).toBe(offset === 0 ? vector.signature : mac(payload));
    expect(sign).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'bitrue',
      wsAuth: { toSign: payload, outputFormat: 'hex' } }));
    expect(server.exchange.options).not.toHaveProperty('secret');
    expect(server.exchange.options).not.toHaveProperty('remoteSigner');
    expect(fallback).not.toHaveBeenCalled();
    expect(browser.io).not.toHaveBeenCalled();
    expect(server.io).not.toHaveBeenCalled();
  }
});

it('actual WASM PayloadSigner preserves callback metadata without overriding native auth headers', async () => {
  const payloadSigner = vi.fn(({ payload }: { payload: string }) => ({ signature: mac(payload),
    headers: { 'x-ch-sign': 'injected', 'Content-Type': 'injected', 'X-Trace': 'fixture' } }));
  const server = fixture({ secret: '', payloadSigner });
  const output = await server.exchange.signHttpRequest(request(vectors[0]));
  expect(output.error).toBeUndefined();
  expect(output.config?.headers).toMatchObject({ 'X-CH-SIGN': vectors[0].signature, 'Content-Type': 'application/json', 'X-Trace': 'fixture' });
  expect(server.io).not.toHaveBeenCalled();
});

it.each(['error', 'malformed', 'unavailable'] as const)('actual WASM refuses %s without secret leakage, network or fallback', async (mode) => {
  const sign = vi.fn(async ({ id }: RemoteSignerRequest) => mode === 'error'
    ? { id, error: `untrusted callback ${secret}` } : { id, wsAuthSignature: '00'.repeat(31) });
  const fallback = vi.fn();
  const server = fixture({ secret: '', remoteSigner: { sign, isAvailable: () => mode !== 'unavailable', onAvailabilityChange: () => () => {} }, payloadSigner: fallback });
  const output = await server.exchange.signHttpRequest(request(vectors[0]));
  expect(output.error).toBeTruthy();
  expect(output.config).toBeUndefined();
  expect(JSON.stringify(output)).not.toContain(secret);
  expect(server.io).not.toHaveBeenCalled();
  expect(fallback).not.toHaveBeenCalled();
});

it.each(['remote', 'payload'] as const)('actual native startup with %s custody authenticates the allowed private lifecycle', async (mode) => {
  const browser = fixture();
  const { sign, remoteSigner } = delegate(browser.exchange);
  const packets: Array<{ url: string; method: string; headers: Record<string, string>; body: string }> = [];
  const sockets: string[] = [];
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp,
    request: async ({ url, method, headers, body }) => {
      packets.push({ url, method, headers, body: typeof body === 'string' ? body : '' });
      const data = url.includes('/contracts')
        ? [{ symbol: 'E-BTC-USDT', pricePrecision: 1, multiplier: 0.001, minOrderVolume: 1, maxMarketVolume: 10000, status: 1 }]
        : url.includes('listenKey') ? { code: 200, data: { listenKey: 'fixture-listen' } }
          : url.includes('/account') ? { code: '0', data: { account: [{ marginCoin: 'USDT', totalEquity: 100,
            accountNormal: 100, accountLock: 0, totalPositionNormal: 0, sumOpenRealizedAmount: 0, positionVos: [] }] } }
            : { code: '0', data: [] };
      return { status: 200, body: new TextEncoder().encode(JSON.stringify(data)) };
    },
    connect: (url, handlers) => { sockets.push(url); queueMicrotask(() => handlers.onOpen()); return { send() {}, close() {} }; },
    ...timers }));
  const server = new RustEngineExchange('bitrue', { key, secret: '', ...(mode === 'remote' ? { remoteSigner }
    : { payloadSigner: ({ payload }) => ({ signature: mac(payload) }) }) }, new DefaultStore());
  live.push(server);
  const errors: unknown[] = [];
  void server.start().catch((error) => errors.push(error));
  await vi.waitFor(() => {
    expect(packets.some((packet) => packet.url.includes('/account'))).toBe(true);
    if (mode === 'remote') expect(sockets).toContain('wss://fapiws.bitrue.com/stream?listenKey=fixture-listen');
  });
  for (const packet of packets) {
    const url = new URL(packet.url);
    expect(packet.headers['X-CH-APIKEY']).toBe(key);
    expect(packet.headers['X-CH-SIGN']).toBe(mac(`${packet.headers['X-CH-TS']}${packet.method}${url.pathname}${url.search}${packet.body}`));
  }
  if (mode === 'remote') expect(sign.mock.calls.some(([input]) => input.wsAuth?.toSign.includes('POST/user_stream/api/v1/listenKey'))).toBe(true);
  else {
    expect(packets.some((packet) => packet.url.includes('listenKey'))).toBe(false);
    expect(sockets.some((url) => url.includes('fapiws.bitrue.com'))).toBe(false);
  }
  expect(errors).toEqual([]);
});
