import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { request, vectors } from '../support/wooHostedSigning';

const stamp = 1_700_000_000_000;
const material = 'fixture-signing-material';
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
  const exchange = new RustEngineExchange('woo', { engine: 'rust', key: 'fixture-public', secret: material,
    applicationId: 'fixture-app', ...options }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
};
const signer = (browser: RustEngineExchange) => {
  const sign = vi.fn((input: RemoteSignerRequest) => browser.signWsAuth(input));
  const remoteSigner: RemoteSigner = { sign, accountName: 'fixture-account', isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return { sign, remoteSigner };
};

it.each(vectors)('actual WASM Woo $label: keyless RemoteSigner matches local payload and signature bytes', async (vector) => {
  const browser = fixture();
  const { sign, remoteSigner } = signer(browser.exchange);
  const fallback = vi.fn(() => { throw new Error('No PayloadSigner fallback'); });
  const server = fixture({ secret: '', remoteSigner, payloadSigner: fallback });
  const local = await browser.exchange.signHttpRequest(request(vector));
  const delegated = await server.exchange.signHttpRequest(request(vector));
  expect(local.error).toBeUndefined(); expect(delegated.error).toBeUndefined();
  expect(delegated.config).toEqual(local.config);
  expect(delegated.config?.headers['x-api-signature']).toBe(vector.signature);
  expect(sign).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'woo',
    accountName: 'fixture-account', wsAuth: { toSign: vector.payload, outputFormat: 'hex' } }));
  expect(server.exchange.options).not.toHaveProperty('secret');
  expect(server.exchange.options).not.toHaveProperty('remoteSigner');
  expect(fallback).not.toHaveBeenCalled();
  expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
});

it('actual WASM Woo WS signing matches the local key and the native auth recipe', async () => {
  const browser = fixture(); const { sign, remoteSigner } = signer(browser.exchange);
  const server = fixture({ secret: '', remoteSigner });
  const input: RemoteSignerRequest = { id: 'auth', type: 'ws-auth', exchangeName: 'woo', wsAuth: { toSign: `|${stamp}`, outputFormat: 'hex' } };
  const local = await browser.exchange.signWsAuth(input);
  expect(await server.exchange.signWsAuth(input)).toEqual(local);
  expect(local).toEqual({ id: 'auth', wsAuthSignature: mac(`|${stamp}`) });
  expect(sign).toHaveBeenCalledOnce();
});

it('provider failures and malformed results cannot leak material, send I/O or fall back', async () => {
  const logs = [vi.spyOn(console, 'error').mockImplementation(() => {}), vi.spyOn(console, 'warn').mockImplementation(() => {}), vi.spyOn(console, 'log').mockImplementation(() => {})];
  for (const mode of ['error', 'malformed', 'unavailable'] as const) {
    const sign = vi.fn(async (input: RemoteSignerRequest) => mode === 'error'
      ? { id: input.id, error: `fixture provider error ${material}` } : { id: input.id, wsAuthSignature: '00'.repeat(31) });
    const fallback = vi.fn(() => { throw new Error('No fallback'); });
    const remoteSigner: RemoteSigner = { sign, isAvailable: () => mode !== 'unavailable', onAvailabilityChange: () => () => {} };
    const server = fixture({ remoteSigner, payloadSigner: fallback });
    const result = await server.exchange.signHttpRequest(request(vectors[8]));
    expect(result.error).toBeTruthy(); expect(result.config).toBeUndefined();
    expect(JSON.stringify(result)).not.toContain(material);
    expect(server.io).not.toHaveBeenCalled(); expect(fallback).not.toHaveBeenCalled();
  }
  expect(JSON.stringify(logs.map((log) => log.mock.calls))).not.toContain(material);
});

it('PayloadSigner remains explicitly refused because Woo has no legacy private PayloadSigner lifecycle', () => {
  let error: unknown;
  try { fixture({ payloadSigner: () => ({ signature: '00'.repeat(32) }) }); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
});

it('actual WASM Woo private login and account reads use the browser provider with no server secret', async () => {
  const browser = fixture(); const { sign, remoteSigner } = signer(browser.exchange);
  const logins: string[] = [];
  const requests: Array<{ url: string; method: string; headers: Record<string, string>; body?: string }> = [];
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp,
    request: async ({ url, method, headers, body }) => {
      requests.push({ url, method, headers, body: typeof body === 'string' ? body : undefined });
      let data: unknown;
      if (url.includes('/public/info')) data = { success: true, rows: [{ symbol: 'PERP_BTC_USDT', base_tick: 0.001,
        quote_tick: 0.1, base_min: 0.001, base_max: 100, base_imr: 0.02, price_range: 0.01 }] };
      else if (url.includes('/public/futures')) data = { success: true, rows: [{ symbol: 'PERP_BTC_USDT', '24h_open': 59000,
        '24h_close': 60000, '24h_volume': 4, '24h_amount': 240000, index_price: 59999, mark_price: 60001,
        last_funding_rate: 0, next_funding_time: stamp + 1000, open_interest: 10000 }] };
      else if (url.includes('/accountinfo')) data = { success: true, data: { accountMode: 'FUTURES', positionMode: 'ONE_WAY',
        freeCollateral: 1000, totalCollateral: 1000, totalAccountValue: 1000 } };
      else if (url.includes('/positions')) data = { success: true, data: { positions: [] } };
      else if (url.includes('/v3/algo/orders')) data = { success: true, data: { rows: [], meta: { currentPage: 1, recordsPerPage: 100, total: 0 } } };
      else data = { success: true, rows: [], meta: { current_page: 1, records_per_page: 100, total: 0 } };
      return { status: 200, body: new TextEncoder().encode(JSON.stringify(data)) };
    },
    connect: (_url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return { close() {}, send(text) {
        if (!text.startsWith('{') || JSON.parse(text).event !== 'auth') return;
        logins.push(text);
        queueMicrotask(() => handlers.onMessage('{"event":"auth","success":true}'));
      } };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const server = new RustEngineExchange('woo', { engine: 'rust', key: 'fixture-public', secret: '',
    applicationId: 'fixture-app', remoteSigner }, new DefaultStore()); live.push(server);
  const errors: unknown[] = [];
  void server.start().catch((error) => errors.push(error));
  await vi.waitFor(() => { expect(logins).toHaveLength(1); expect(requests.some((r) => r.headers['x-api-signature'])).toBe(true); });
  expect(JSON.parse(logins[0])).toEqual({ event: 'auth', params: { apikey: 'fixture-public', sign: mac(`|${stamp}`), timestamp: stamp } });
  for (const r of requests.filter((r) => r.headers['x-api-signature'])) {
    const url = new URL(r.url);
    const payload = url.pathname.includes('/v1/') ? `${url.search.slice(1)}|${stamp}` : `${stamp}${r.method}${url.pathname}${url.search}${r.body ?? ''}`;
    expect(r.headers['x-api-signature']).toBe(mac(payload));
    expect(r.headers['x-api-key']).toBe('fixture-public');
  }
  expect(sign.mock.calls.some(([input]) => input.wsAuth?.toSign === `|${stamp}`)).toBe(true);
  expect(errors).toEqual([]);
});
