import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';

import { afterEach, expect, it, vi } from 'vitest';

import vectors from '../../../../../sdk-rs/venues/cryptocom/tests/fixtures/hosted-signing.json';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { generateSignature } from '../reference/cryptocom/cryptocom.signing';
import { prepareRecordedHttp } from '../support/managedHttp';

let stamp = 1_610_000_000_000;
const live: RustEngineExchange[] = [];
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
  vi.restoreAllMocks();
  stamp = 1_610_000_000_000;
});
const fixture = (options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => {
    throw new Error('Signing must not use network');
  });
  setRustEngineTransport(() => ({
    prepareHttp: prepareRecordedHttp,
    now: () => stamp,
    request: io,
    connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const exchange = new RustEngineExchange(
    'cryptocom',
    { engine: 'rust', key: 'replay-key', secret: 'replay-secret', ...options },
    new DefaultStore(),
  );
  live.push(exchange);
  return { exchange, io };
};
const remote = (device: RustEngineExchange) => {
  const sign = vi.fn((request: RemoteSignerRequest) => device.signWsAuth(request));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return { sign, remoteSigner };
};

it.each(vectors)('actual WASM Crypto.com $method: keyless hosted signing matches the device wire', async (v) => {
  const device = fixture();
  const { sign, remoteSigner } = remote(device.exchange);
  const fallback = vi.fn(() => {
    throw new Error('No fallback');
  });
  const origin = fixture({ secret: '', remoteSigner, payloadSigner: fallback });
  for (const now of [stamp, stamp + 500]) {
    stamp = now;
    const request: RemoteSignerRequest = {
      id: v.method,
      type: 'http',
      exchangeName: 'cryptocom',
      config: {
        method: 'POST',
        url: `/${v.method}`,
        baseURL: 'https://api.crypto.com/exchange/v1',
        headers: {},
        data: v.params,
      },
    };
    const local = await device.exchange.signHttpRequest(request);
    const hosted = await origin.exchange.signHttpRequest(request);
    expect(local.error).toBeUndefined();
    expect(hosted.error).toBeUndefined();
    expect(hosted.config).toEqual(local.config);
    const payload = `${v.method}${stamp}${v.payload.slice(v.method.length + 13, -13)}${stamp}`;
    expect(JSON.parse(hosted.config!.data as string).sig).toBe(
      createHmac('sha256', 'replay-secret').update(payload).digest('hex'),
    );
    expect(sign).toHaveBeenLastCalledWith(
      expect.objectContaining({
        type: 'ws-auth',
        exchangeName: 'cryptocom',
        wsAuth: { toSign: payload, outputFormat: 'hex' },
      }),
    );
  }
  expect(origin.exchange.options).not.toHaveProperty('secret');
  expect(origin.exchange.options).not.toHaveProperty('remoteSigner');
  expect(JSON.stringify(sign.mock.calls)).not.toContain('replay-secret');
  expect(fallback).not.toHaveBeenCalled();
  expect(origin.io).not.toHaveBeenCalled();
  expect(device.io).not.toHaveBeenCalled();
});

it('actual WASM WS authentication uses the remote provider and a matching receipt', async () => {
  const device = fixture();
  const { sign, remoteSigner } = remote(device.exchange);
  const origin = fixture({ secret: '', remoteSigner });
  const request: RemoteSignerRequest = {
    id: 'ws',
    type: 'ws-auth',
    exchangeName: 'cryptocom',
    wsAuth: {
      toSign: `public/auth${stamp}replay-key${stamp}`,
      outputFormat: 'hex',
    },
  };
  const local = await device.exchange.signWsAuth(request);
  expect(await origin.exchange.signWsAuth(request)).toEqual(local);
  expect(local.wsAuthSignature).toBe('57a5207e795bd876080625ee876fa750cd0134c3916bcdc346d4efe1b6f0218f');
  expect(sign).toHaveBeenCalledOnce();
  expect(origin.io).not.toHaveBeenCalled();
});

it('actual WASM PayloadSigner receives only canonical HMAC input', async () => {
  const payloadSigner = vi.fn(async ({ payload }: { payload: string }) => ({
    signature: createHmac('sha256', 'replay-secret').update(payload).digest('hex'),
    headers: { 'X-Trace': 'keep' },
  }));
  const origin = fixture({ secret: '', payloadSigner });
  const result = await origin.exchange.signHttpRequest({
    id: 'payload',
    type: 'http',
    exchangeName: 'cryptocom',
    config: {
      url: '/private/get-accounts',
      method: 'POST',
      baseURL: 'https://api.crypto.com/exchange/v1',
      headers: {},
      data: {},
    },
  });
  expect(result.error).toBeUndefined();
  expect(result.config?.headers['X-Trace']).toBe('keep');
  expect(payloadSigner).toHaveBeenCalledWith({
    exchangeName: 'cryptocom',
    key: 'replay-key',
    payload: `private/get-accounts${stamp}replay-key${stamp}`,
    scheme: 'hmac-sha256',
    encoding: 'hex',
  });
  expect(origin.io).not.toHaveBeenCalled();
});

it.each(['remote', 'payload'] as const)(
  'actual WASM %s startup signs native reconciliation and respects private streaming policy',
  async (mode) => {
    const device = fixture();
    await device.exchange.signWsAuth({
      id: 'initialize-device',
      type: 'ws-auth',
      exchangeName: 'cryptocom',
      wsAuth: { toSign: 'initialize', outputFormat: 'hex' },
    });
    const { sign, remoteSigner } = remote(device.exchange);
    const payloadSigner = vi.fn(async ({ payload }: { payload: string }) => ({
      signature: createHmac('sha256', 'replay-secret').update(payload).digest('hex'),
      headers: { 'X-Trace': 'keep' },
    }));
    const requests: { url: string; body?: string; headers: Record<string, string> }[] = [];
    const sockets: string[] = [];
    const logins: string[] = [];
    let placementStatus = 200;
    const began = Date.now();
    setRustEngineTransport(() => ({
      prepareHttp: prepareRecordedHttp,
      now: () => stamp + Date.now() - began,
      request: async ({ url, body, headers }) => {
        requests.push({ url, body: typeof body === 'string' ? body : undefined, headers });
        let data: unknown = { code: 0, result: { data: [] } };
        if (url.endsWith('/public/get-instruments'))
          data = {
            code: 0,
            result: {
              data: [
                {
                  symbol: 'BTCUSD-PERP',
                  inst_type: 'PERPETUAL_SWAP',
                  tradable: true,
                  base_ccy: 'BTC',
                  quote_ccy: 'USD',
                  price_tick_size: '0.1',
                  qty_tick_size: '0.001',
                  quote_decimals: 1,
                  quantity_decimals: 3,
                  max_leverage: '50',
                  underlying_symbol: 'BTCUSD-INDEX',
                },
              ],
            },
          };
        if (url.endsWith('/public/get-tickers'))
          data = {
            code: 0,
            result: { data: [{ i: 'BTCUSD-PERP', b: '99', k: '101', a: '100', c: '0.01', v: '1', vv: '100' }] },
          };
        if (url.endsWith('/private/user-balance'))
          data = {
            code: 0,
            result: {
              data: [
                {
                  total_margin_balance: '1000',
                  total_available_balance: '950',
                  total_initial_margin: '50',
                  total_session_unrealized_pnl: '10',
                },
              ],
            },
          };
        if (url.endsWith('/private/create-order')) {
          return {
            status: placementStatus,
            body: new TextEncoder().encode(JSON.stringify({ code: 0, result: { order_id: '12345' } })),
          };
        }
        return { status: 200, body: new TextEncoder().encode(JSON.stringify(data)) };
      },
      connect: (url, handlers) => {
        sockets.push(url);
        queueMicrotask(() => handlers.onOpen());
        return {
          close() {},
          send(text) {
            if (JSON.parse(text).method !== 'public/auth') return;
            logins.push(text);
            queueMicrotask(() => handlers.onMessage('{"method":"public/auth","code":0}'));
          },
        };
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    }));
    const exchange = new RustEngineExchange(
      'cryptocom',
      { engine: 'rust', key: 'replay-key', secret: '', ...(mode === 'remote' ? { remoteSigner } : { payloadSigner }) },
      new DefaultStore(),
    );
    live.push(exchange);
    const errors: unknown[] = [];
    void exchange.start().catch((error) => errors.push(error));
    await vi.waitFor(
      () => {
        expect(requests.filter((r) => r.url.includes('/private/')).length).toBeGreaterThanOrEqual(4);
        if (mode === 'remote') expect(logins).toHaveLength(1);
        expect(exchange.store.loaded.positions).toBe(true);
      },
      { timeout: 4000 },
    );
    if (mode === 'payload') {
      expect(sockets.some((url) => url.endsWith('/user'))).toBe(false);
      expect(logins).toHaveLength(0);
      expect(payloadSigner).toHaveBeenCalled();
      expect(sign).not.toHaveBeenCalled();
    } else {
      const auth = JSON.parse(logins[0]);
      expect(auth.sig).toBe(
        createHmac('sha256', 'replay-secret').update(`public/auth${auth.id}replay-key${auth.nonce}`).digest('hex'),
      );
      expect(sign.mock.calls.some(([request]) => request.wsAuth?.toSign.startsWith('public/auth'))).toBe(true);
    }
    const order = {
      symbol: 'BTCUSD-PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.01,
      price: 100,
      clientOrderId: 'metadata-order',
    };
    expect(await exchange.placeOrder(order)).toEqual(['12345']);
    placementStatus = 503;
    await expect(exchange.placeOrder({ ...order, clientOrderId: 'metadata-unknown' })).rejects.toMatchObject({
      outcome: 'unknown',
      message: expect.stringContaining('unknown'),
    });
    expect(requests.filter((r) => r.url.endsWith('/private/create-order'))).toHaveLength(2);
    for (const request of requests.filter((r) => r.url.includes('/private/'))) {
      const envelope = JSON.parse(request.body!);
      expect(envelope.api_key).toBe('replay-key');
      expect(envelope.sig).toBe(
        generateSignature(envelope.method, envelope.id, 'replay-key', 'replay-secret', envelope.params, envelope.nonce),
      );
      if (mode === 'payload') expect(request.headers['X-Trace']).toBe('keep');
      expect(request.headers).not.toHaveProperty('sig');
    }
    expect(errors).toEqual([]);
  },
);

it.each(['throw', 'malformed', 'unavailable'])(
  'actual WASM refuses %s signer output without secret fallback',
  async (mode) => {
    const sign = vi.fn(async (request: RemoteSignerRequest) => {
      if (mode === 'throw') throw new Error('replay-secret');
      return { id: request.id, wsAuthSignature: '00'.repeat(31) };
    });
    const payloadSigner = vi.fn();
    const origin = fixture({
      remoteSigner: { sign, isAvailable: () => mode !== 'unavailable', onAvailabilityChange: () => () => {} },
      payloadSigner,
    });
    const result = await origin.exchange.signWsAuth({
      id: 'bad',
      type: 'ws-auth',
      exchangeName: 'cryptocom',
      wsAuth: { toSign: 'canonical', outputFormat: 'hex' },
    });
    expect(result.error).toBeTruthy();
    expect(result.wsAuthSignature).toBeUndefined();
    expect(JSON.stringify(result)).not.toContain('replay-secret');
    expect(payloadSigner).not.toHaveBeenCalled();
    expect(origin.io).not.toHaveBeenCalled();
  },
);
