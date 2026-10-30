import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { InternalAxiosRequestConfig } from 'axios';
import type { BaseExchange } from '../exchanges/base';
import type { ExchangeName, ExchangeOptions } from '../types';

import { Blob as NodeBlob } from 'node:buffer';
import { createHash, createHmac } from 'node:crypto';
import { deflateSync, gzipSync } from 'node:zlib';

import { isRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { createEngineObject } from '@tealstreet/safe-cex-wasm/workers';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createApiWithSecondaryKey } from '../createApiWithSecondaryKey';
import { accountToExchangeOptions } from '../createExchangeFromAccount';
import { exchangeMetadata } from '../exchangeMetadata';
import { createExchangeAPI, createInfoAPI } from '../__tests__/reference/hyperliquid/hyperliquid.api';
import { createAPI as createSpotAPI } from '../__tests__/reference/hyperliquid/hyperliquidspot.api';
import { createAPI as createKrakenSpotAPI } from '../__tests__/reference/kraken/krakenspot.api';
import { createAPI as createParadexAPI } from '../__tests__/reference/paradex/paradex.api';
import { createAPI as createKucoinAPI } from '../__tests__/reference/kucoin/kucoin.api';
import { createKucoinSpotAPI } from '../__tests__/reference/kucoin/kucoinspot.api';
import { directTransport } from './direct';
import { createEngineTransport, resolveEngineRoutingPlan } from './engine';
import { setDefaultTransport } from './registry';
import { resolveExchangeRouting } from './tealstreet/routing/resolveExchangeRouting';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import {
  PUBLIC_CORS_PROXY,
  PRIVATE_PROXY_ASIA,
  PRIVATE_PROXY_GRVT_TINY,
  PRIVATE_PROXY_JP,
} from './tealstreet/transportResolution';
import { setWsRelayEnabled } from './tealstreet/wsRelay';

const mocks = vi.hoisted(() => ({
  token: vi.fn(),
  adapter: vi.fn(),
  createAdapter: vi.fn(),
  relay: vi.fn(),
  nodeRuntime: vi.fn(() => true),
}));

vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: mocks.token,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  // The real binder looks the token up through getProxyToken.
  bindProxyTokenLookup: () => () => mocks.token(),
}));
vi.mock('../index', () => ({ createExchange: vi.fn() }));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: mocks.nodeRuntime }));
// Avoid global HOW socket/token configuration. The real provider still selects
// and configures the adapter; its returned adapter captures the final request.
vi.mock('../httpOverWebsocket/axiosHttpOverWebsocketAdapter', () => ({
  axiosHttpOverWebsocketAdapter: mocks.adapter,
  bigIntAxiosHttpOverWebsocketAdapter: mocks.adapter,
  createAxiosHttpOverWebsocketAdapter: mocks.createAdapter,
}));
vi.mock('@tealstreet/http-over-websocket/src/RelayWebSocket', () => ({ RelayWebSocket: mocks.relay }));

type Request = Parameters<Transport['request']>[0];

const makeRaw = () => ({
  webSocketHeaders: true,
  now: vi.fn(() => 42),
  request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
  connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
  setTimeout: vi.fn<Transport['setTimeout']>(),
  clearTimeout: vi.fn<Transport['clearTimeout']>(),
});

const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'venue-key',
  secret: 'venue-secret',
  transport: tealstreetTransport,
  ...extra,
});

const request = (url = 'https://api.bybit.com/v5/order/create?timestamp=123&signature=a%2Fb'): Request => ({
  method: 'POST',
  url,
  headers: { 'X-BAPI-API-KEY': 'venue-key', 'Content-Type': 'application/json' },
  body: '{"orderId":9007199254740993123,"price":"1.2300"}',
});

const browserHeaders = {
  Origin: 'https://trade.tealstreet.io',
  Referer: 'https://trade.tealstreet.io/',
  'User-Agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
  'sec-fetch-mode': 'cors',
  'sec-fetch-site': 'cross-site',
  'sec-fetch-dest': 'empty',
};

beforeEach(() => {
  mocks.nodeRuntime.mockReturnValue(true);
  mocks.token.mockResolvedValue('proxy-jwt');
  mocks.adapter.mockImplementation(async (config: InternalAxiosRequestConfig) => ({
    status: 200,
    statusText: 'OK',
    headers: { 'content-type': 'application/json', 'x-native': 'untouched' },
    config,
    data: '{"orderId":9007199254740993123,"price":1.2300}',
  }));
  mocks.createAdapter.mockReturnValue(mocks.adapter);
});

afterEach(() => {
  vi.useRealTimers();
  setDefaultTransport(directTransport);
  setWsRelayEnabled(false);
  vi.unstubAllGlobals();
});

describe('engine HTTP routing at the raw transport boundary', () => {
  it.each(['coinbase', 'coinbasespot'] as const)('preserves %s venue JWT separately from REST proxy authorization', async (exchange) => {
    const raw = makeRaw();
    const opts = options({ tealstreetProxy: true, tealstreetTunnel: false });
    const input = { ...request('https://api.coinbase.com/api/v3/brokerage/orders?cursor=a%2Fb'), headers: { authorization: 'Bearer venue-jwt', 'Content-Type': 'application/json' } };
    await createEngineTransport(exchange, opts, raw).request(input);
    const output = raw.request.mock.lastCall![0];
    expect(output.headers['Coinbase-Authorization']).toBe('Bearer venue-jwt');
    expect(output.headers.Authorization).toBe('Bearer proxy-jwt');
    expect(output.headers.authorization).toBeUndefined();
    expect(output.url).toBe(`${PRIVATE_PROXY_ASIA}/${input.url}`);
    expect(output.body).toBe(input.body);
    expect(input.headers.authorization).toBe('Bearer venue-jwt');
    const plan = resolveEngineRoutingPlan(exchange, opts, 'https://api.coinbase.com');
    expect(plan.http.headerAliases).toEqual({ Authorization: 'Coinbase-Authorization' });
    expect(JSON.stringify(plan)).not.toMatch(/venue-jwt|proxy-jwt|venue-secret/);
  });

  it.each(['coinbase', 'coinbasespot'] as const)('keeps standard %s Authorization for direct and HOW routes', async (exchange) => {
    for (const mode of ['direct', 'HOW'] as const) {
      const raw = makeRaw();
      const opts = options({ tealstreetProxy: mode === 'HOW', tealstreetTunnel: mode === 'HOW' });
      const input = { ...request('https://api.coinbase.com/api/v3/brokerage/orders'), headers: { Authorization: 'Bearer venue-jwt' } };
      await createEngineTransport(exchange, opts, raw).request(input);
      const headers = mode === 'HOW' ? mocks.adapter.mock.lastCall![0].headers.toJSON() : raw.request.mock.lastCall![0].headers;
      expect(headers.Authorization).toBe('Bearer venue-jwt');
      expect(headers['Coinbase-Authorization']).toBeUndefined();
      expect(resolveEngineRoutingPlan(exchange, opts, 'https://api.coinbase.com').http.headerAliases).toBeUndefined();
    }
  });

  it('does not invent Coinbase venue authorization for unsigned public proxy requests', async () => {
    const raw = makeRaw();
    await createEngineTransport('coinbase', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw)
      .request({ ...request('https://api.international.coinbase.com/api/v1/instruments/'), headers: {} });
    expect(raw.request.mock.lastCall![0].headers['Coinbase-Authorization']).toBeUndefined();
    expect(raw.request.mock.lastCall![0].headers.Authorization).toBe('Bearer proxy-jwt');
  });

  it('fails closed on conflicting Coinbase proxy aliases before token acquisition or network I/O', async () => {
    const raw = makeRaw();
    await expect(createEngineTransport('coinbase', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw)
      .request({ ...request('https://api.coinbase.com/api/v3/brokerage/orders'), headers: { Authorization: 'Bearer venue-jwt', 'Coinbase-Authorization': 'Bearer conflicting-jwt' } }))
      .rejects.toThrow('header alias conflicts');
    expect(mocks.token).not.toHaveBeenCalled();
    expect(raw.request).not.toHaveBeenCalled();
  });

  it('rejects proxy authorization that replaces Coinbase venue credential bytes', async () => {
    const raw = makeRaw();
    const provider = {
      ...directTransport,
      rest: () => ({ proxied: true, authorize: async (config: InternalAxiosRequestConfig) => {
        config.headers.set('Coinbase-Authorization', 'Bearer overwritten-venue-jwt');
        config.headers.set('Authorization', 'Bearer proxy-jwt');
      } }),
    };
    await expect(createEngineTransport('coinbase', options({ transport: provider, tealstreetTunnel: false }), raw)
      .request({ ...request('https://api.coinbase.com/api/v3/brokerage/orders'), headers: { Authorization: 'Bearer venue-jwt' } }))
      .rejects.toThrow('changed signed request bytes or headers');
    expect(raw.request).not.toHaveBeenCalled();
  });
  it('never sends a proxy write after disposal while its token was pending', async () => {
    let token!: (value: string) => void;
    mocks.token.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          token = resolve;
        }),
    );
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw);
    const pending = routed.request(request());
    const rejected = expect(pending).rejects.toThrow('disposed');
    routed.dispose!();
    await rejected;
    token('late-jwt');
    await Promise.resolve();
    expect(raw.request).not.toHaveBeenCalled();
  });

  it('includes proxy token acquisition in the request deadline and never sends a stale write', async () => {
    vi.useFakeTimers();
    let token!: (value: string) => void;
    mocks.token.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          token = resolve;
        }),
    );
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw);
    const rejected = expect(routed.request(request())).rejects.toThrow('timed out');
    await vi.advanceTimersByTimeAsync(20_000);
    await rejected;
    token('late-jwt');
    await Promise.resolve();
    expect(raw.request).not.toHaveBeenCalled();
    routed.dispose!();
  });

  it('releases the deadline when disposed before a response stream is consumed', async () => {
    vi.useFakeTimers();
    const raw = makeRaw();
    raw.request.mockResolvedValue({
      status: 200,
      body: (async function* () {
        yield new Uint8Array([1]);
      })(),
    });
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), raw);
    await routed.request(request());
    expect(vi.getTimerCount()).toBe(1);
    routed.dispose!();
    expect(vi.getTimerCount()).toBe(0);
    expect(raw.request.mock.lastCall![0].signal?.aborted).toBe(true);
  });
  it('does not let signed query values select a different proxy pool', async () => {
    const raw = makeRaw();
    const input = request('https://api.bybit.com/v5/order/create?clientId=binance&a=1&a=2&signature=a%2Fb');
    await createEngineTransport('bybit', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw).request(
      input,
    );
    expect(raw.request.mock.lastCall![0].url).toBe(`${PRIVATE_PROXY_ASIA}/${input.url}`);
    expect(raw.request.mock.lastCall![0].body).toBe(input.body);
  });
  it('supplies proxy fetch headers in a server runtime without Node globals', async () => {
    mocks.nodeRuntime.mockReturnValue(false);
    const raw = { ...makeRaw(), serverHeaders: true };
    await createEngineTransport('okx', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw).request(
      request(),
    );
    expect(raw.request.mock.lastCall![0].headers).toMatchObject({
      ...browserHeaders,
      Authorization: 'Bearer proxy-jwt',
    });
  });

  it('passes the host raw socket constructor to HOW and disposes its adapter', async () => {
    const WebSocket = class {} as unknown as typeof globalThis.WebSocket;
    const dispose = vi.fn();
    const adapter = Object.assign(async (config: InternalAxiosRequestConfig) => mocks.adapter(config), { dispose });
    mocks.createAdapter.mockReturnValue(adapter);
    const raw = { ...makeRaw(), WebSocket, dispose: vi.fn() };
    const routed = createEngineTransport('bybit', options({ tealstreetTunnel: true }), raw);
    await routed.request(request());
    expect(mocks.createAdapter).toHaveBeenCalledWith({ region: 'SG', customParser: expect.any(Function), WebSocket });
    routed.dispose!();
    expect(dispose).toHaveBeenCalledOnce();
    expect(raw.dispose).toHaveBeenCalledOnce();
  });
  it('keeps direct URLs, signed bodies and venue credentials intact', async () => {
    const raw = makeRaw();
    const input = request();
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), raw);
    await routed.request(input);
    expect(raw.request).toHaveBeenCalledExactlyOnceWith({ ...input, signal: expect.any(AbortSignal) });
    expect(mocks.token).not.toHaveBeenCalled();
    expect(mocks.createAdapter).not.toHaveBeenCalled();
    expect(routed.now()).toBe(42);
  });

  it('uses the runtime provider and lets an account override it', async () => {
    const raw = makeRaw();
    const rest = vi.fn(({ config }) => ({ baseURL: `https://custom.invalid/${config.baseURL}`, proxied: false }));
    const wsRouting = vi.fn(directTransport.wsRouting);
    setDefaultTransport({ ...directTransport, name: 'custom', rest, wsRouting });
    await createEngineTransport('bybit', { key: 'k', secret: 's' }, raw).request(request());
    expect(raw.request.mock.calls[0][0].url).toBe(`https://custom.invalid/${request().url}`);
    // Bybit also resolves its inverse and spot product public routes.
    expect(wsRouting.mock.calls.map(([name]) => name)).toEqual(['bybit', 'bybitinverse', 'bybitspot']);
    await createEngineTransport('bybit', options({ transport: directTransport }), raw).request(request());
    expect(raw.request.mock.calls[1][0]).toEqual({ ...request(), signal: expect.any(AbortSignal) });
    expect(rest).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['bybit', undefined, PRIVATE_PROXY_ASIA],
    ['bybit', 'JA', PRIVATE_PROXY_JP],
    ['bybit', 'SG', PRIVATE_PROXY_ASIA],
    ['okx', undefined, PRIVATE_PROXY_JP],
    ['bitget', 'SG', PRIVATE_PROXY_JP],
    ['grvt', 'JA', PRIVATE_PROXY_GRVT_TINY],
  ] as const)('routes %s region=%s through its exact CORS pool', async (exchange, proxyRegion, prefix) => {
    const raw = makeRaw();
    const input = request(`https://${exchange}.example/api?signature=%2B%2F`);
    if (exchange === 'grvt') input.headers.Authorization = 'venue-session';
    await createEngineTransport(
      exchange,
      options({ tealstreetProxy: true, tealstreetTunnel: false, proxyRegion }),
      raw,
    ).request(input);
    expect(raw.request).toHaveBeenCalledExactlyOnceWith({
      ...input,
      signal: expect.any(AbortSignal),
      url: `${prefix}/${input.url}`,
      headers: {
        ...input.headers,
        ...browserHeaders,
        [exchange === 'grvt' ? 'Tealstreet-Authorization' : 'Authorization']: 'Bearer proxy-jwt',
      },
    });
    expect(mocks.createAdapter).not.toHaveBeenCalled();
  });

  it('resolves the proxy token for each operation, waiting out an initially empty credential', async () => {
    vi.useFakeTimers();
    try {
      mocks.token
        .mockResolvedValueOnce('')
        .mockResolvedValueOnce('refreshed-jwt')
        .mockResolvedValueOnce('rotated-jwt');
      const raw = makeRaw();
      const routed = createEngineTransport('bybit', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw);
      const first = routed.request(request());
      await vi.advanceTimersByTimeAsync(1_000);
      await first;
      await routed.request(request());
      // The warming '' never reached the edge; each operation read its own token.
      expect(raw.request).toHaveBeenCalledTimes(2);
      expect(raw.request.mock.calls[0][0].headers.Authorization).toBe('Bearer refreshed-jwt');
      expect(raw.request.mock.calls[1][0].headers.Authorization).toBe('Bearer rotated-jwt');
      expect(mocks.token).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it.each([false, true])('overrideProxyUrl rewrites the venue URL with proxy=%s', async (tealstreetProxy) => {
    const raw = makeRaw();
    const input = request();
    await createEngineTransport(
      'bybit',
      options({ tealstreetProxy, tealstreetTunnel: false, overrideProxyUrl: 'https://override.invalid/proxy' }),
      raw,
    ).request(input);
    expect(raw.request.mock.calls[0][0].url).toBe(`https://override.invalid/proxy/${input.url}`);
    expect(raw.request.mock.calls[0][0].headers).toEqual(
      tealstreetProxy ? { ...input.headers, ...browserHeaders, Authorization: 'Bearer proxy-jwt' } : input.headers,
    );
    expect(mocks.token).toHaveBeenCalledTimes(tealstreetProxy ? 1 : 0);
  });

  it('keeps Binance public requests direct but sends unsigned listen-key requests through the private pool', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('binance', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw);
    const publicRequest = { ...request('https://fapi.binance.com/fapi/v1/exchangeInfo'), headers: {} };
    const listenKey = {
      ...request('https://fapi.binance.com/fapi/v1/listenKey'),
      headers: { 'x-mbx-apikey': 'venue-key' },
    };
    await routed.request(publicRequest);
    await routed.request(listenKey);
    expect(raw.request.mock.calls[0][0]).toEqual({ ...publicRequest, signal: expect.any(AbortSignal) });
    expect(raw.request.mock.calls[1][0]).toEqual({
      ...listenKey,
      signal: expect.any(AbortSignal),
      url: `${PRIVATE_PROXY_JP}/${listenKey.url}`,
      headers: { ...listenKey.headers, ...browserHeaders, Authorization: 'Bearer proxy-jwt' },
    });
    expect(mocks.token).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['bybit', 'JA', 'JA'],
    ['bybit', 'SG', 'SG'],
    ['binance', 'SG', 'JA'],
    ['phemex', 'JA', 'SG'],
  ] as const)(
    'HOW %s region=%s uses the resolved %s egress without parsing signed JSON',
    async (exchange, proxyRegion, region) => {
      vi.useFakeTimers();
      const raw = makeRaw();
      const input = request(`https://${exchange}.example/order?signature=a%2Fb`);
      const response = await createEngineTransport(
        exchange,
        options({ tealstreetProxy: true, tealstreetTunnel: true, proxyRegion }),
        raw,
      ).request(input);
      expect(mocks.createAdapter).toHaveBeenCalledExactlyOnceWith({ region, customParser: expect.any(Function) });
      const parser = mocks.createAdapter.mock.calls[0][0].customParser;
      expect(parser('{"id":9007199254740993123}')).toBe('{"id":9007199254740993123}');
      expect(raw.request).not.toHaveBeenCalled();
      expect(mocks.adapter).toHaveBeenCalledExactlyOnceWith(
        expect.objectContaining({
          method: input.method,
          url: input.url,
          baseURL: input.url,
          data: input.body,
          timeout: 20_000,
        }),
      );
      expect(mocks.adapter.mock.calls[0][0].headers.toJSON()).toEqual(input.headers);
      expect(new TextDecoder().decode(response.body as Uint8Array)).toBe(
        '{"orderId":9007199254740993123,"price":1.2300}',
      );
      expect(response.headers).toBe('content-type: application/json\nx-native: untouched');
      expect(mocks.token).not.toHaveBeenCalled();
    },
  );

  it.each([
    [undefined, false, true],
    [false, false, false],
    [true, false, true],
    [undefined, true, false],
    [true, true, false],
  ] as const)(
    'BingX tunnel=%s publicOnly=%s preserves its explicit-false opt out',
    async (tealstreetTunnel, publicDataOnly, tunnelled) => {
      const raw = makeRaw();
      await createEngineTransport('bingx', options({ tealstreetTunnel, publicDataOnly }), raw).request(
        request('https://open-api.bingx.com/openApi/swap/v3/quote/klines'),
      );
      expect(mocks.adapter).toHaveBeenCalledTimes(tunnelled ? 1 : 0);
      expect(raw.request).toHaveBeenCalledTimes(tunnelled ? 0 : 1);
    },
  );

  it.each(['grvt', 'blofin'] as const)(
    '%s uses its forced private proxy when the account asks for HOW',
    async (exchange) => {
      const raw = makeRaw();
      const input = request(`https://${exchange}.example/orders`);
      await createEngineTransport(exchange, options({ tealstreetTunnel: true, tealstreetProxy: false }), raw).request(
        input,
      );
      expect(raw.request.mock.calls[0][0].url).toBe(
        `${exchange === 'grvt' ? PRIVATE_PROXY_GRVT_TINY : PRIVATE_PROXY_ASIA}/${input.url}`,
      );
      expect(mocks.adapter).not.toHaveBeenCalled();
    },
  );

  it.each(['weex', 'mexc'] as const)('%s never installs HOW, matching its TS API factory', async (exchange) => {
    const raw = makeRaw();
    await createEngineTransport(exchange, options({ tealstreetTunnel: true }), raw).request(
      request(
        exchange === 'weex'
          ? 'https://api-contract.weex.com/capi/v2/account/assets'
          : 'https://api.mexc.com/api/v1/private/account/assets',
      ),
    );
    expect(raw.request).toHaveBeenCalledTimes(1);
    expect(mocks.createAdapter).not.toHaveBeenCalled();
  });

  it('keeps MEXC venue Authorization separate from the private proxy credential', async () => {
    const raw = makeRaw();
    await createEngineTransport('mexc', options({ tealstreetProxy: true, tealstreetTunnel: false }), raw).request({
      ...request('https://api.mexc.com/api/v1/private/order/list/open_orders'),
      headers: { Authorization: 'venue-session' },
    });
    expect(raw.request.mock.lastCall![0]).toMatchObject({
      url: `${PRIVATE_PROXY_JP}/https://api.mexc.com/api/v1/private/order/list/open_orders`,
      headers: { Authorization: 'venue-session', 'Tealstreet-Authorization': 'Bearer proxy-jwt' },
    });
  });

  it.each([
    ['direct', false, false],
    ['proxy', true, false],
    ['HOW', true, true],
  ] as const)('preserves Breakout DXAPI session authorization through %s routing', async (_label, tealstreetProxy, tealstreetTunnel) => {
    const raw = makeRaw();
    const url = 'https://breakout.prosp.devexperts.com/dxsca-web/accounts/default%3A00002/portfolio';
    const input = { method: 'GET', url, headers: { Authorization: 'DXAPI documented-session' } };
    const opts = options({ tealstreetProxy, tealstreetTunnel });
    const routed = createEngineTransport('breakoutprop', opts, raw);
    try {
      await routed.request(input);
      const delivered = tealstreetTunnel ? mocks.adapter.mock.lastCall![0] : raw.request.mock.lastCall![0];
      const headers = tealstreetTunnel ? delivered.headers.toJSON() : delivered.headers;
      expect(headers.Authorization).toBe('DXAPI documented-session');
      expect(headers['Tealstreet-Authorization']).toBe(tealstreetProxy && !tealstreetTunnel ? 'Bearer proxy-jwt' : undefined);
      expect(headers['Coinbase-Authorization']).toBeUndefined();
      const plan = resolveEngineRoutingPlan('breakoutprop', opts, 'https://breakout.prosp.devexperts.com');
      expect(plan.http.authorizationHeader).toBe('Tealstreet-Authorization');
      expect(plan.http.authorize).toBe(tealstreetProxy && !tealstreetTunnel);
      expect(plan.http.mode).toBe(tealstreetTunnel ? 'how' : 'direct');
      if (tealstreetTunnel) expect(raw.request).not.toHaveBeenCalled();
    } finally { routed.dispose!(); }
  });

  it('fails closed if a tunnel adapter returns parsed JSON', async () => {
    mocks.adapter.mockResolvedValueOnce({ status: 200, headers: {}, data: { id: 9007199254740992 } });
    await expect(
      createEngineTransport('bybit', options({ tealstreetTunnel: true }), makeRaw()).request(request()),
    ).rejects.toThrow('raw response body');
  });

  it.each(Object.keys(exchangeMetadata) as ExchangeName[])(
    '%s account preferences reach the TS golden HTTP route',
    async (exchange) => {
      // Both golden suites independently freeze account normalization and the
      // resolver's full routing matrix. Drive their real outputs through the host
      // rather than snapshotting a second copy of the routing table here.
      for (const tealstreetProxy of [false, true]) {
        for (const tealstreetTunnel of [false, true]) {
          for (const proxyRegion of [undefined, 'JA', 'SG']) {
            const resolved = accountToExchangeOptions(
              {
                exchange,
                name: 'routing-matrix',
                apiKey: 'key',
                apiSecret: 'secret',
                tealstreetProxy,
                tealstreetTunnel,
                proxyRegion,
              },
              { transport: tealstreetTransport },
            );
            const routing = resolveExchangeRouting(exchange, resolved);
            const raw = makeRaw();
            const input = {
              ...request(`https://${exchange}.example/engine-routing`),
              headers: { 'X-MBX-APIKEY': 'key' },
            };
            mocks.adapter.mockClear();
            mocks.createAdapter.mockClear();
            const routed = createEngineTransport(exchange, resolved, raw);
            await routed.request(input);
            const how =
              ['hyperliquid', 'hyperliquidspot', 'kucoin', 'kucoinspot'].includes(exchange)
                ? !resolved.publicDataOnly
                : exchange === 'bingx'
                  ? resolved.tealstreetTunnel !== false
                  : !['grvt', 'blofin', 'weex', 'mexc'].includes(exchange) && Boolean(resolved.tealstreetTunnel);
            const label = `${exchange} proxy=${tealstreetProxy} tunnel=${tealstreetTunnel} region=${proxyRegion}`;
            expect(routed.httpMode, label).toBe(how ? 'how' : 'direct');
            expect(mocks.adapter.mock.calls.length, label).toBe(how ? 1 : 0);
            if (how) {
              expect(mocks.createAdapter.mock.calls[0][0].region, label).toBe(routing.howEgress === 'jp' ? 'JA' : 'SG');
              expect(mocks.adapter.mock.calls[0][0].url, label).toBe(input.url);
            } else {
              const forceProxy = (exchange === 'grvt' || exchange === 'blofin') && resolved.tealstreetTunnel;
              const effectiveRouting = forceProxy
                ? resolveExchangeRouting(exchange, { ...resolved, tealstreetProxy: true })
                : routing;
              const prefix = resolved.tealstreetTunnel && !forceProxy ? '' : effectiveRouting.restProxyHost;
              expect(raw.request.mock.calls[0][0].url, label).toBe(prefix ? `${prefix}/${input.url}` : input.url);
              expect(raw.request.mock.calls[0][0].body, label).toBe(input.body);
            }
          }
        }
      }
    },
  );
});

const websocketHeaders = {
  Origin: 'https://preview.tealstreet.io',
  'User-Agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36',
};

const handlers = (channel: 'public' | 'private' | 'business' = 'public') => ({
  channel,
  onOpen: vi.fn(),
  onMessage: vi.fn(),
  onClose: vi.fn(),
});

describe('engine socket routing at the raw transport boundary', () => {
  it.each(Object.keys(exchangeMetadata) as ExchangeName[])(
    '%s routing permutations open exactly the TS channel host',
    (exchange) => {
      for (const tealstreetProxy of [false, true]) {
        for (const tealstreetWsRoute of [false, true]) {
          for (const proxyRegion of [undefined, 'JA', 'SG']) {
            for (const bybitSiteId of [undefined, 'BRA_BTL'] as const) {
              const opts = options({
                tealstreetProxy,
                tealstreetWsRoute,
                proxyRegion,
                bybitSiteId,
                tealstreetTunnel: false,
              });
              const routing = resolveExchangeRouting(exchange, opts);
              const raw = makeRaw();
              const routed = createEngineTransport(exchange, opts, raw);
              for (const [channel, route] of [
                ['public', routing.wsPublic],
                ['private', routing.wsPrivate],
                ['business', routing.wsBusiness],
              ] as const) {
                // Deliberately opaque: role must come from the engine's explicit
                // channel rather than a URL-name heuristic or routed host.
                const upstream = `wss://venue.example/socket-${channel}`;
                const handler = handlers(channel);
                const socket = routed.connect(upstream, handler);
                // Bitunix TS classes explicitly select fallback on this option.
                const target = exchange === 'bitunix' && tealstreetWsRoute
                  ? route.fallback! : route.tunnelled ? route.host! : upstream;
                const label = `${exchange} ${channel} proxy=${tealstreetProxy} ws=${tealstreetWsRoute} region=${proxyRegion} site=${bybitSiteId}`;
                // Since AWS_EXIT.md step 3e a proxied okx/okxeu/bitget/blofin private
                // host IS a relay url, which RelayWebSocket dials, never raw.connect.
                if (isRelayUrl(target)) {
                  expect(raw.connect, label).not.toHaveBeenCalled();
                  socket.close();
                  continue;
                }
                const expectedHeaders = {
                  ...(['bybit', 'bybitinverse', 'bybitspot'].includes(exchange) && bybitSiteId
                    ? { 'x-site-id': bybitSiteId }
                    : {}),
                  ...(new URL(target).hostname.endsWith('.tealstreet.io') ? websocketHeaders : {}),
                };
                expect(raw.connect.mock.lastCall?.[0], label).toBe(target);
                expect(raw.connect.mock.lastCall?.[1].headers, label).toEqual(expectedHeaders);
                raw.connect.mockClear();
              }
            }
          }
        }
      }
      expect(mocks.token).not.toHaveBeenCalled();
      expect(mocks.relay).not.toHaveBeenCalled();
    },
  );

  it('preserves native venue upgrade headers and adds WAF headers on a routed socket', () => {
    const raw = makeRaw();
    const handler = {
      ...handlers('private'),
      headers: { 'X-Venue-Auth': 'signed-upgrade', Authorization: 'venue-auth' },
    };
    createEngineTransport('bybit', options({ bybitSiteId: 'BRA_BTL' }), raw).connect(
      'wss://stream.bybit.com/v5/private',
      handler,
    );
    // Was fsjnpy143pbz.asia (AWS) until AWS_EXIT.md step 3e.
    expect(raw.connect).toHaveBeenCalledExactlyOnceWith(
      'wss://fsjnpy143pbz-tyo.tealstreet.io/bybitbra/v5/private',
      expect.objectContaining({ headers: { ...handler.headers, 'x-site-id': 'BRA_BTL', ...websocketHeaders } }),
    );
  });

  // 'browser sockets select the same host without attempting unsupported WAF upgrade
  // headers' was removed with the last browser ws-proxy route: coinbase moved to the
  // relay (AWS_EXIT.md 5a) after okx/okxeu/bitget/blofin (3e), so no proxied private
  // socket is dialled through raw.connect any more.

  it('two silent connections select the sticky fallback while any message resets the streak', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', options({ tealstreetProxy: false }), raw);
    const url = 'wss://stream.bybit.com/v5/private';
    const connect = () => routed.connect(url, handlers('private'));
    connect();
    connect();
    raw.connect.mock.lastCall![1].onMessage('{"success":false,"ret_msg":"wrong key"}');
    connect();
    connect();
    expect(raw.connect.mock.lastCall?.[0]).toBe(url);
    connect();
    expect(raw.connect.mock.lastCall?.[0]).toBe('wss://bybit-proxy-ja.tealstreet.io/v5/private');
    raw.connect.mock.lastCall![1].onMessage('pong');
    connect();
    expect(raw.connect.mock.lastCall?.[0]).toBe('wss://bybit-proxy-ja.tealstreet.io/v5/private');
  });

  it('tracks channel fallback lifetimes separately even when upstream URLs match', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('okx', options(), raw);
    const url = 'wss://venue.example/shared-socket';
    routed.connect(url, handlers('public'));
    routed.connect(url, handlers('private'));
    routed.connect(url, handlers('business'));
    expect(raw.connect.mock.calls.map(([target]) => target)).toEqual([url, url, url]);
  });

  it('gate-enabled private sockets try relay before the passthrough and buffer sends until open', async () => {
    setWsRelayEnabled(true);
    const listeners = new Map<string, (event?: unknown) => void>();
    const relay = {
      addEventListener: vi.fn((name: string, listener: (event?: unknown) => void) => listeners.set(name, listener)),
      send: vi.fn(),
      close: vi.fn(),
    };
    mocks.relay.mockImplementation(function () {
      return relay;
    });
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', options({ proxyRegion: 'JA' }), raw);
    const url = 'wss://stream.bybit.com/v5/private';
    routed.connect(url, handlers('private'));
    routed.connect(url, handlers('private'));
    const handler = handlers('private');
    const socket = routed.connect(url, handler);
    socket.send('{"op":"auth"}');
    await vi.waitFor(() => expect(mocks.relay).toHaveBeenCalledTimes(1));
    // Was fsjnpy143pbz.jp (AWS) until AWS_EXIT.md step 3e.
    expect(mocks.relay).toHaveBeenCalledWith(
      'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fstream.bybit.com%2Fv5%2Fprivate',
      [],
      { headers: websocketHeaders, ALPNProtocols: ['http/1.1'] },
    );
    expect(raw.connect).toHaveBeenCalledTimes(2);
    expect(relay.send).not.toHaveBeenCalled();
    socket.send('queued-after-construction');
    handler.onOpen.mockImplementation(() => socket.send('on-open-write'));
    expect(relay.send).not.toHaveBeenCalled();
    listeners.get('open')!();
    expect(handler.onOpen).toHaveBeenCalledOnce();
    expect(relay.send.mock.calls).toEqual([['{"op":"auth"}'], ['queued-after-construction'], ['on-open-write']]);
    listeners.get('message')!({ data: 'pong' });
    expect(handler.onMessage).toHaveBeenCalledExactlyOnceWith('pong');
    listeners.get('close')!({ code: 1001 });
    expect(handler.onClose).toHaveBeenCalledExactlyOnceWith(1001);
    socket.send('after-close');
    expect(relay.send).toHaveBeenCalledTimes(3);
    const retries = [
      routed.connect(url, handlers('private')),
      routed.connect(url, handlers('private')),
      routed.connect(url, handlers('private')),
    ];
    expect(raw.connect.mock.lastCall?.[0]).toBe('wss://bybit-proxy-ja.tealstreet.io/v5/private');
    // One probe may still be awaiting its dynamic relay constructor. Retire it
    // before Vitest resets the mock; it must not open a socket in the next test.
    retries.forEach((retry) => retry.close());
  });
});

describe('native host policy uses the same resolved routes', () => {
  it('rejects incomplete secondary credentials before producing a native plan or a routed transport', () => {
    const opts = options({ secondary_key: 'read-key' });
    expect(() => resolveEngineRoutingPlan('binance', opts, 'https://fapi.binance.com')).toThrow(
      'secondary signing keys',
    );
    expect(() => createEngineTransport('binance', opts, makeRaw())).toThrow('secondary signing keys');
    expect(mocks.token).not.toHaveBeenCalled();
  });
  it('is serializable, token-free, and matches the actual CORS/WS destinations', async () => {
    const opts = options({ tealstreetProxy: true, tealstreetTunnel: false, proxyRegion: 'JA' });
    const input = request('https://www.okx.com/api/v5/account/balance');
    const plan = resolveEngineRoutingPlan('okx', opts, 'https://www.okx.com');
    expect(JSON.parse(JSON.stringify(plan))).toEqual(plan);
    expect(JSON.stringify(plan)).not.toMatch(/proxy-jwt|venue-key|venue-secret/);
    expect(mocks.token).not.toHaveBeenCalled();
    expect(plan.http).toEqual({
      mode: 'direct',
      prefix: PRIVATE_PROXY_JP,
      authorize: true,
      authorizationHeader: 'Authorization',
      headers: browserHeaders,
    });
    // Was the AWS ws-proxy `fsjnpy143pbz.asia…/okx/private` until AWS_EXIT.md step 3e.
    expect(plan.ws.private.host).toBe(
      'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fws.okx.com%2Fws%2Fv5%2Fprivate',
    );
    const raw = makeRaw();
    await createEngineTransport('okx', opts, raw).request(input);
    expect(raw.request.mock.lastCall?.[0].url).toBe(`${plan.http.prefix}/${input.url}`);
    expect(plan.wsHeaders).toEqual(websocketHeaders);
  });

  // Both regions were fsjnpy143pbz.{jp,asia} (AWS) until AWS_EXIT.md step 3e.
  it.each([
    ['JA', 'fsjnpy143pbz-tyo'],
    ['SG', 'fsjnpy143pbz-tyo'],
  ] as const)('HOW region %s supplies exact native tunnel host', (proxyRegion, host) => {
    const plan = resolveEngineRoutingPlan(
      'bybit',
      options({ tealstreetTunnel: true, proxyRegion }),
      'https://api.bybit.com',
    );
    expect(plan.http.mode).toBe('how');
    expect(plan.http.tunnelUrl).toBe(`wss://${host}.tealstreet.io/tunnel`);
    expect(plan.http.prefix).toBe('');
    expect(plan.http.authorize).toBe(false);
    expect(mocks.token).not.toHaveBeenCalled();
  });

  it('Binance native public/private policies mirror the keyless versus listen-key split', () => {
    const plan = resolveEngineRoutingPlan(
      'binance',
      options({ tealstreetProxy: true, tealstreetTunnel: false }),
      'https://fapi.binance.com',
    );
    expect(plan.publicHeader).toBe('X-MBX-APIKEY');
    expect(plan.publicHttp).toMatchObject({ mode: 'direct', prefix: '', authorize: false });
    expect(plan.http).toMatchObject({ mode: 'direct', prefix: PRIVATE_PROXY_JP, authorize: true });
  });
});

describe('Bybit Brazil routing carries the site identity on every transport', () => {
  it.each([
    ['direct', false, false],
    ['proxy', true, false],
    ['HOW', true, true],
  ] as const)('%s REST and the native plan preserve x-site-id', async (_mode, tealstreetProxy, tealstreetTunnel) => {
    const opts = options({ bybitSiteId: 'BRA_BTL', tealstreetProxy, tealstreetTunnel });
    const raw = makeRaw();
    await createEngineTransport('bybit', opts, raw).request(request());
    const headers = tealstreetTunnel
      ? mocks.adapter.mock.lastCall![0].headers.toJSON()
      : raw.request.mock.lastCall![0].headers;
    expect(headers['x-site-id']).toBe('BRA_BTL');
    const plan = resolveEngineRoutingPlan('bybit', opts, 'https://api.bybit.com');
    expect(plan.http.headers['x-site-id']).toBe('BRA_BTL');
    expect(plan.venueWsHeaders).toEqual({ 'x-site-id': 'BRA_BTL' });
  });

  it('browser Brazil sockets choose the injecting proxy without unsupported headers', () => {
    const raw = { ...makeRaw(), webSocketHeaders: false };
    createEngineTransport('bybit', options({ bybitSiteId: 'BRA_BTL' }), raw).connect(
      'wss://stream.bybit.com/v5/private',
      handlers('private'),
    );
    // Was fsjnpy143pbz.asia (AWS) until AWS_EXIT.md step 3e.
    expect(raw.connect.mock.lastCall![0]).toBe('wss://fsjnpy143pbz-tyo.tealstreet.io/bybitbra/v5/private');
    expect(raw.connect.mock.lastCall![1].headers).toEqual({});
  });

  it('Bybit demo excludes the Brazil route and site header', async () => {
    const opts = options({ bybitSiteId: 'BRA_BTL', tealstreetTunnel: false });
    const raw = makeRaw();
    const routed = createEngineTransport('bybitdemo', opts, raw);
    await routed.request(request('https://api-demo.bybit.com/v5/account/wallet-balance'));
    routed.connect('wss://stream-demo.bybit.com/v5/private', handlers('private'));
    expect(raw.request.mock.lastCall![0].headers).not.toHaveProperty('x-site-id');
    expect(raw.connect.mock.lastCall![0]).toBe('wss://stream-demo.bybit.com/v5/private');
    expect(raw.connect.mock.lastCall![1].headers).toEqual({});
  });
});

const connectedRelay = async () => {
  setWsRelayEnabled(true);
  const listeners = new Map<string, (event?: unknown) => void>();
  const relay = {
    addEventListener: vi.fn((name: string, listener: (event?: unknown) => void) => listeners.set(name, listener)),
    send: vi.fn(),
    close: vi.fn(),
  };
  mocks.relay.mockImplementation(function () {
    return relay;
  });
  const routed = createEngineTransport('bybit', options(), makeRaw());
  const url = 'wss://stream.bybit.com/v5/private';
  routed.connect(url, handlers('private'));
  routed.connect(url, handlers('private'));
  const handler = handlers('private');
  const socket = routed.connect(url, handler);
  await vi.waitFor(() => expect(listeners.has('message')).toBe(true));
  return { relay, handler, socket, emit: (data: unknown) => listeners.get('message')!({ data }), listeners };
};

describe('relay binary frames obey the native transport text contract', () => {
  it('decodes gzip Blob, zlib bytes, UTF-8 buffers and following text in arrival order', async () => {
    // happy-dom's Blob lacks the stream method used by the production decoder.
    vi.stubGlobal('Blob', NodeBlob);
    const { emit, handler } = await connectedRelay();
    emit(new NodeBlob([gzipSync('{"n":1}')]));
    emit('second');
    emit(deflateSync('{"n":3}'));
    emit(new TextEncoder().encode('fourth').buffer);
    emit('fifth');
    await vi.waitFor(() =>
      expect(handler.onMessage.mock.calls).toEqual([['{"n":1}'], ['second'], ['{"n":3}'], ['fourth'], ['fifth']]),
    );
    expect(handler.onClose).not.toHaveBeenCalled();
  });

  it('malformed compression closes the relay and reports one failure despite queued frames', async () => {
    vi.stubGlobal('Blob', NodeBlob);
    const { emit, handler, relay, listeners } = await connectedRelay();
    emit(new Uint8Array([0x1f, 0x8b, 0]));
    emit('after-malformed-frame');
    emit(new Uint8Array([0x78, 0xff, 0]));
    await vi.waitFor(() => expect(relay.close).toHaveBeenCalledExactlyOnceWith());
    listeners.get('close')!({ code: 1007 });
    expect(handler.onClose).toHaveBeenCalledExactlyOnceWith(1007);
    expect(relay.close).toHaveBeenCalledOnce();
    expect(handler.onMessage).not.toHaveBeenCalled();
  });

  it('close during an asynchronous Blob decode suppresses it and all queued callbacks', async () => {
    vi.stubGlobal('Blob', NodeBlob);
    let finish!: (bytes: ArrayBuffer) => void;
    const bytes = new Promise<ArrayBuffer>((resolve) => {
      finish = resolve;
    });
    const arrayBuffer = vi.fn(() => bytes);
    class DelayedBlob extends NodeBlob {
      override arrayBuffer(): Promise<ArrayBuffer> {
        return arrayBuffer();
      }
    }
    const { emit, handler, socket, relay } = await connectedRelay();
    emit(new DelayedBlob(['pending']));
    emit('queued-text');
    await vi.waitFor(() => expect(arrayBuffer).toHaveBeenCalledOnce());
    socket.close();
    finish(new TextEncoder().encode('pending').buffer);
    await bytes;
    // Let the already-started decode and the following queued text drain.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(relay.close).toHaveBeenCalledOnce();
    expect(handler.onMessage).not.toHaveBeenCalled();
    expect(handler.onClose).not.toHaveBeenCalled();
  });
});

describe('Bitget host routing keeps engine feeds and recovery independent', () => {
  const publicUrl = 'wss://ws.bitget.com/v2/ws/public';
  const privateUrl = 'wss://ws.bitget.com/v2/ws/private';
  const utaUrl = 'wss://ws.bitget.com/v3/ws/private';
  const withContext = (feed: string, recover = false, channel: 'public' | 'private' = 'public') => ({
    ...handlers(channel),
    context: { feed, recover },
  });
  // The private fallback was the AWS ws-proxy `fsjnpy143pbz.asia…/bitget/v2/private`,
  // dialled through raw.connect, until AWS_EXIT.md step 3e made it the relay itself,
  // which RelayWebSocket dials asynchronously.
  const relayV2 = 'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fws.bitget.com%2Fv2%2Fws%2Fprivate';
  const relayDials = () => mocks.relay.mock.calls.map(([url]) => url as string);
  beforeEach(() => {
    mocks.relay.mockImplementation(function () {
      return { addEventListener: vi.fn(), send: vi.fn(), close: vi.fn() };
    });
  });

  it('starts all three same-URL public feeds directly and advances each feed independently', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options(), raw);
    for (const feed of ['tickers', 'books', 'candles']) routed.connect(publicUrl, withContext(feed));
    expect(raw.connect.mock.calls.map(([url]) => url)).toEqual([publicUrl, publicUrl, publicUrl]);
    routed.connect(publicUrl, withContext('tickers'));
    expect(raw.connect.mock.lastCall![0]).toBe('wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public');
    // A working books feed remains direct while its sibling has fallen back.
    raw.connect.mock.calls[1][1].onMessage('pong');
    routed.connect(publicUrl, withContext('books'));
    expect(raw.connect.mock.lastCall![0]).toBe(publicUrl);
    routed.connect(publicUrl, withContext('candles'));
    expect(raw.connect.mock.lastCall![0]).toBe('wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public');
  });

  it.each(['bitget', 'bitgetinverse', 'bitgetusdc'] as const)(
    '%s classic post-write recovery selects and sticks to the declared private fallback',
    async (exchange) => {
      const raw = makeRaw();
      const routed = createEngineTransport(exchange, options(), raw);
      routed.connect(privateUrl, withContext('account', false, 'private'));
      raw.connect.mock.lastCall![1].onMessage('{"event":"login","code":"0"}');
      routed.connect(privateUrl, withContext('account', true, 'private'));
      await vi.waitFor(() => expect(relayDials()).toEqual([relayV2]));
      routed.connect(privateUrl, withContext('account', false, 'private'));
      await vi.waitFor(() => expect(relayDials()).toEqual([relayV2, relayV2]));
      expect(raw.connect).toHaveBeenCalledOnce();
    },
  );

  it('the direct provider keeps recovery on the real venue host', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options({ transport: directTransport }), raw);
    for (let attempt = 0; attempt < 5; attempt++) routed.connect(privateUrl, withContext('account', true, 'private'));
    expect(raw.connect.mock.calls.every(([url]) => url === privateUrl)).toBe(true);
  });

  it('UTA private sockets take the v3 privateUta route and never a v2 route', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options({ tealstreetProxy: true }), raw);
    // A proxied socket's every candidate is the v3 relay since AWS_EXIT.md step 3e.
    // One dial at a time: concurrent dynamic imports of the mocked relay module can
    // resolve the real one under Vitest.
    for (let attempt = 1; attempt <= 10; attempt++) {
      routed.connect(utaUrl, withContext('account', true, 'private'));
      await vi.waitFor(() => expect(relayDials()).toHaveLength(attempt));
    }
    expect(raw.connect).not.toHaveBeenCalled();
    expect(relayDials().some((url) => url.includes('%2Fv2%2F'))).toBe(false);
    expect(relayDials().every((url) => url.includes('%2Fv3%2F'))).toBe(true);
  });

  const rejecting = (recover = false) => ({
    ...handlers('private'),
    context: { feed: 'account', recover, rejectRoute: true },
    onRouteExhausted: vi.fn(),
  });

  it('rejectRoute advances exactly one route, even with recover set', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options(), raw);
    routed.connect(privateUrl, withContext('account', false, 'private'));
    expect(raw.connect.mock.lastCall![0]).toBe(privateUrl);
    routed.connect(privateUrl, rejecting(true));
    await vi.waitFor(() => expect(relayDials()).toEqual([relayV2]));
    expect(raw.connect).toHaveBeenCalledOnce();
  });

  it('an exhausted chain reports after connect returns, never dials, and the next connect stays on the last route', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options(), raw);
    // Since AWS_EXIT.md step 3e the later routes are relay dials; every socket is
    // retired at the end so a pending relay import cannot open one in the next test.
    const sockets = [routed.connect(privateUrl, withContext('account', false, 'private'))];
    const handler = rejecting();
    let dials = raw.connect.mock.calls.length;
    for (let attempt = 0; attempt < 5 && raw.connect.mock.calls.length === dials + attempt; attempt++) {
      sockets.push(routed.connect(privateUrl, handler));
    }
    dials = raw.connect.mock.calls.length;
    const last = raw.connect.mock.lastCall![0];
    const exhausted = rejecting();
    sockets.push(routed.connect(privateUrl, exhausted));
    expect(raw.connect).toHaveBeenCalledTimes(dials);
    expect(exhausted.onRouteExhausted).not.toHaveBeenCalled();
    raw.setTimeout.mock.lastCall![0]();
    expect(exhausted.onRouteExhausted).toHaveBeenCalledOnce();
    sockets.push(routed.connect(privateUrl, withContext('account', false, 'private')));
    // The last route is the relay, so staying on it never reaches raw.connect again.
    expect(raw.connect).toHaveBeenCalledTimes(dials);
    expect(raw.connect.mock.lastCall![0]).toBe(last);
    sockets.forEach((socket) => socket.close());
  });

  it('a direct provider has no other egress, so a refusal exhausts at once', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options({ transport: directTransport }), raw);
    routed.connect(privateUrl, withContext('account', false, 'private'));
    const exhausted = rejecting();
    routed.connect(privateUrl, exhausted);
    expect(raw.connect).toHaveBeenCalledOnce();
    expect(exhausted.onRouteExhausted).not.toHaveBeenCalled();
    raw.setTimeout.mock.lastCall![0]();
    expect(exhausted.onRouteExhausted).toHaveBeenCalledOnce();
  });

  it('closing an exhausted connect before its report suppresses the report', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options({ transport: directTransport }), raw);
    routed.connect(privateUrl, withContext('account', false, 'private'));
    const exhausted = rejecting();
    routed.connect(privateUrl, exhausted).close();
    raw.setTimeout.mock.lastCall![0]();
    expect(exhausted.onRouteExhausted).not.toHaveBeenCalled();
  });

  it.each([
    ['pong', true],
    ['{"event":"login","code":"0"}', false],
  ] as const)('private frame %s counts toward stream proof=%s', async (message, fallbackExpected) => {
    const raw = makeRaw();
    const routed = createEngineTransport('bitget', options(), raw);
    routed.connect(privateUrl, withContext('account', false, 'private'));
    raw.connect.mock.lastCall![1].onMessage(message);
    routed.connect(privateUrl, withContext('account', false, 'private'));
    if (fallbackExpected) {
      await vi.waitFor(() => expect(relayDials()).toEqual([relayV2]));
      expect(raw.connect).toHaveBeenCalledOnce();
    } else {
      expect(raw.connect.mock.lastCall![0]).toBe(privateUrl);
      expect(raw.connect).toHaveBeenCalledTimes(2);
    }
  });
});

it('a recovery hint preserves direct routing when the only candidate is a relay', () => {
  setWsRelayEnabled(true);
  const raw = makeRaw();
  const provider = {
    ...tealstreetTransport,
    wsRouting: (exchange: ExchangeName, opts: Partial<ExchangeOptions>) => {
      const routing = tealstreetTransport.wsRouting(exchange, opts);
      return { ...routing, wsPrivate: { ...routing.wsPrivate, fallback: undefined } };
    },
  };
  createEngineTransport('bybit', options({ transport: provider }), raw).connect('wss://stream.bybit.com/v5/private', {
    ...handlers('private'),
    context: { feed: 'account', recover: true },
  });
  expect(raw.connect.mock.lastCall![0]).toBe('wss://stream.bybit.com/v5/private');
  expect(mocks.relay).not.toHaveBeenCalled();
});

describe('Hyperliquid uses the factories original publicDataOnly option', () => {
  const permutations = ([undefined, false, true] as const).flatMap((publicDataOnly) =>
    ([undefined, false, true] as const).flatMap((tealstreetTunnel) =>
      (['hyperliquid', 'hyperliquidspot'] as const).map((exchange) => ({ exchange, publicDataOnly, tealstreetTunnel })),
    ),
  );

  it.each(permutations)(
    '$exchange publicOnly=$publicDataOnly tunnel=$tealstreetTunnel matches actual TS factories',
    async ({ exchange, publicDataOnly, tealstreetTunnel }) => {
      const opts = options({ publicDataOnly, tealstreetTunnel });
      const parent = { name: exchange } as BaseExchange;
      // These are the production factories, not a copied decision expression:
      // both perps clients and the spot client install HOW from the raw flag.
      const clients =
        exchange === 'hyperliquid'
          ? [createExchangeAPI(opts, parent), createInfoAPI(opts, parent)]
          : [createSpotAPI(opts, parent)];
      const factoryUsesHow = clients[0].defaults.adapter === mocks.adapter;
      expect(clients.every((client) => (client.defaults.adapter === mocks.adapter) === factoryUsesHow)).toBe(true);
      const raw = makeRaw();
      const host = createEngineTransport(exchange, opts, raw);
      await host.request({ ...request('https://api.hyperliquid.xyz/info'), body: '{"type":"meta"}' });
      await host.request({ ...request('https://api.hyperliquid.xyz/exchange'), body: '{"action":{"type":"cancel"}}' });
      expect(mocks.adapter).toHaveBeenCalledTimes(factoryUsesHow ? 2 : 0);
      expect(raw.request).toHaveBeenCalledTimes(factoryUsesHow ? 0 : 2);
      expect(resolveEngineRoutingPlan(exchange, opts, 'https://api.hyperliquid.xyz').http.mode).toBe(
        factoryUsesHow ? 'how' : 'direct',
      );
    },
  );

  it.each([
    { vault: '0x1111111111111111111111111111111111111111' },
    { applicationId: '0x2222222222222222222222222222222222222222' },
    {},
  ])('address-only or keyless options retain the raw undefined public flag: %j', async (identity) => {
    const opts = options({ key: '', secret: '', tealstreetTunnel: false, ...identity });
    expect(opts.publicDataOnly).toBeUndefined();
    const parent = { name: 'hyperliquid' } as BaseExchange;
    expect(createInfoAPI(opts, parent).defaults.adapter).toBe(mocks.adapter);
    expect(createExchangeAPI(opts, parent).defaults.adapter).toBe(mocks.adapter);
    const raw = makeRaw();
    await createEngineTransport('hyperliquid', opts, raw).request({
      ...request('https://api.hyperliquid.xyz/info'),
      body: '{"type":"meta"}',
    });
    expect(mocks.adapter).toHaveBeenCalledTimes(1);
    expect(raw.request).not.toHaveBeenCalled();
    expect(resolveEngineRoutingPlan('hyperliquid', opts, 'https://api.hyperliquid.xyz').http.mode).toBe('how');
  });
});


describe('Workers derives GRVT protocol options from its account transport', () => {
  it.each([true, false])('uses the actual private route when proxied=%s even if the caller flag conflicts', async (proxied) => {
    let captured: Record<string, unknown> | undefined;
    let metadata: Transport['engineOptions'];
    const EngineObject = createEngineObject({
      accountToken: () => 'account-fixture-token',
      loadEngine: async () => ({
        WasmEngine: class {
          constructor(_venue: string, json: string) { captured = JSON.parse(json); }
          start() { return ''; }
        } as unknown as new (venue: string, json: string) => import('@tealstreet/safe-cex-wasm/host').WasmEngineLike,
      }),
      createTransport: (venue: string, accountOptions: Record<string, unknown>, raw: Transport) => {
        const routed = createEngineTransport(venue as ExchangeName, {
          ...accountOptions, transport: proxied ? tealstreetTransport : directTransport,
        } as ExchangeOptions, raw);
        metadata = routed.engineOptions;
        return routed;
      },
    });
    const object = new EngineObject({ storage: { setAlarm() {} } });
    const client = { send() {} };
    (object as unknown as { clients: Set<typeof client> }).clients.add(client);
    try {
      await object.receive(client, JSON.stringify({ op: 'start', venue: 'grvt', options: {
        key: 'venue-key', secret: 'venue-secret', grvtTradingAccountId: '777', grvtSessionProxy: !proxied,
      } }));
      expect(captured?.grvtSessionProxy).toBe(proxied);
      expect(captured?.privateWsHeaders).toBe(true);
      expect(captured?.key).toBe('venue-key');
      expect(metadata).toEqual({ grvtSessionProxy: proxied });
    } finally {
      object.leave(client);
    }
  });
});

describe('Paradex matches the production API factory adapter gate', () => {
  const permutations = ([undefined, false, true] as const).flatMap((publicDataOnly) =>
    ([undefined, false, true] as const).flatMap((tealstreetTunnel) =>
      [undefined, async () => ({ signature: 'delegated' })].map((payloadSigner) => ({ publicDataOnly, tealstreetTunnel, payloadSigner }))),
  );

  it.each(permutations)('publicOnly=$publicDataOnly tunnel=$tealstreetTunnel', async (flags) => {
    const opts = options({ ...flags, paradexAddress: '0x123', subkeyAddress: '0x456', secret: '' });
    const client = createParadexAPI(opts, { name: 'paradex' } as BaseExchange);
    const factoryUsesHow = client.defaults.adapter === mocks.adapter;
    const raw = makeRaw();
    const host = createEngineTransport('paradex', opts, raw);
    await host.request({
      ...request('https://api.prod.paradex.trade/v1/orders'),
      headers: {
        Authorization: 'Bearer venue-jwt',
        'Content-Type': 'application/json',
      },
    });
    expect(mocks.adapter).toHaveBeenCalledTimes(factoryUsesHow ? 1 : 0);
    expect(raw.request).toHaveBeenCalledTimes(factoryUsesHow ? 0 : 1);
    expect(resolveEngineRoutingPlan('paradex', opts, 'https://api.prod.paradex.trade/v1').http.mode).toBe(
      factoryUsesHow ? 'how' : 'direct',
    );
    const socketUrl = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false';
    for (const channel of ['public', 'private'] as const) {
      host.connect(socketUrl, { channel, onOpen: vi.fn(), onMessage: vi.fn(), onClose: vi.fn() });
    }
    expect(raw.connect.mock.calls.map(([url]) => url)).toEqual([socketUrl, socketUrl]);
    host.dispose?.();
  });
});

describe('KuCoin matches the production API factories adapter gate', () => {
  const permutations = (['kucoin', 'kucoinspot'] as const).flatMap((exchange) =>
    ([undefined, false, true] as const).flatMap((publicDataOnly) =>
      ([undefined, false, true] as const).flatMap((tealstreetTunnel) =>
        [false, true].map((secondary) => ({ exchange, publicDataOnly, tealstreetTunnel, secondary }))),
    ),
  );
  it.each(permutations)('$exchange publicOnly=$publicDataOnly tunnel=$tealstreetTunnel secondary=$secondary', async ({ exchange, secondary, ...flags }) => {
    const opts = options({ ...flags, passphrase: 'passphrase',
      ...(secondary ? { secondary_key: 'read-key', secondary_secret: 'read-secret' } : {}),
    });
    const factory = exchange === 'kucoin' ? createKucoinAPI : createKucoinSpotAPI;
    const client = factory(opts, { name: exchange } as BaseExchange);
    const factoryUsesHow = client.defaults.adapter === mocks.adapter;
    const raw = makeRaw();
    const base = exchange === 'kucoin' ? 'https://api-futures.kucoin.com' : 'https://api.kucoin.com';
    const host = createEngineTransport(exchange, opts, raw);
    await host.request(request(`${base}/api/v1/accounts`));
    expect(mocks.adapter).toHaveBeenCalledTimes(factoryUsesHow ? 1 : 0);
    expect(raw.request).toHaveBeenCalledTimes(factoryUsesHow ? 0 : 1);
    expect(resolveEngineRoutingPlan(exchange, opts, base).http.mode).toBe(factoryUsesHow ? 'how' : 'direct');
    host.dispose?.();
  });
});

it('keeps the Paradex venue bearer separate from proxy authorization on both host plans', async () => {
  const account = options({ publicDataOnly: false, tealstreetProxy: true, tealstreetTunnel: false });
  const raw = makeRaw();
  const routed = createEngineTransport('paradex', account, raw);
  await routed.request({
    method: 'GET',
    url: 'https://api.prod.paradex.trade/v1/account',
    headers: { Authorization: 'Bearer venue-jwt' },
  });
  const config = mocks.adapter.mock.lastCall![0] as InternalAxiosRequestConfig;
  expect(config.headers.get('Authorization')).toBe('Bearer venue-jwt');
  expect(config.headers.get('Tealstreet-Authorization')).toBe('Bearer proxy-jwt');
  expect(
    resolveEngineRoutingPlan('paradex', account, 'https://api.prod.paradex.trade/v1').http.authorizationHeader,
  ).toBe('Tealstreet-Authorization');
  routed.dispose!();
});

describe('Aster secondary signer HTTP routing', () => {
  const baseURL = 'https://fapi.asterdex.com';
  const privateRequest = (method: string): Request => ({
    method,
    url: `${baseURL}/fapi/v3/order?quantity=0.0010&signature=exact%2Bbytes`,
    headers: { 'x-mbx-apikey': 'read-wallet' },
  });

  it.each(['GET', 'get'])(
    'routes signed %s to the secondary CORS egress and every write to primary',
    async (method) => {
      const opts = options({
        tealstreetProxy: true,
        tealstreetTunnel: false,
        secondary_key: 'read-wallet',
        secondary_secret: 'read-private-key',
      });
      const raw = makeRaw();
      const routed = createEngineTransport('asterdex', opts, raw);
      await routed.request(privateRequest(method));
      expect(raw.request.mock.lastCall![0].url).toBe(`${PUBLIC_CORS_PROXY}/${privateRequest(method).url}`);
      expect(raw.request.mock.lastCall![0].headers['x-mbx-apikey']).toBe('read-wallet');
      for (const write of ['POST', 'PUT', 'DELETE']) {
        await routed.request(privateRequest(write));
        expect(raw.request.mock.lastCall![0].url).toBe(`${PRIVATE_PROXY_ASIA}/${privateRequest(write).url}`);
      }
      await routed.request({ ...privateRequest('GET'), headers: {} });
      expect(raw.request.mock.lastCall![0].url).toBe(`${PRIVATE_PROXY_ASIA}/${privateRequest('GET').url}`);
      routed.dispose!();
    },
  );

  it('retains the primary HOW path and puts secondary CORS inside only its read RPC', async () => {
    const opts = options({
      tealstreetTunnel: true,
      secondary_key: 'read-wallet',
      secondary_secret: 'read-private-key',
    });
    const raw = makeRaw();
    const routed = createEngineTransport('asterdex', opts, raw);
    await routed.request(privateRequest('GET'));
    expect(mocks.adapter.mock.lastCall![0].url).toBe(`${PUBLIC_CORS_PROXY}/${privateRequest('GET').url}`);
    await routed.request(privateRequest('POST'));
    expect(mocks.adapter.mock.lastCall![0].url).toBe(privateRequest('POST').url);
    expect(raw.request).not.toHaveBeenCalled();
    const plan = resolveEngineRoutingPlan('asterdex', opts, baseURL);
    expect(plan.http).toMatchObject({ mode: 'how', prefix: '', authorize: false });
    expect(plan.readHttp).toMatchObject({ mode: 'how', prefix: PUBLIC_CORS_PROXY, authorize: false });
    expect(plan.readHttp?.tunnelUrl).toBe(plan.http.tunnelUrl);
    expect(JSON.stringify(plan)).not.toMatch(/read-wallet|read-private-key|venue-key|venue-secret|proxy-jwt/);
    routed.dispose!();
  });

  it('honors a secondary proxy override while preserving primary-only defaults', async () => {
    const raw = makeRaw();
    const opts = options({
      tealstreetProxy: true,
      tealstreetTunnel: false,
      secondary_key: 'read-wallet',
      secondary_secret: 'read-private-key',
      secondaryProxyUrl: 'https://secondary.example/proxy',
    });
    const routed = createEngineTransport('asterdex', opts, raw);
    await routed.request(privateRequest('GET'));
    expect(raw.request.mock.lastCall![0].url).toBe(`https://secondary.example/proxy/${privateRequest('GET').url}`);
    const plan = resolveEngineRoutingPlan('asterdex', opts, baseURL);
    expect(plan.readHttp?.prefix).toBe('https://secondary.example/proxy');
    expect(plan.http.prefix).toBe(PRIVATE_PROXY_ASIA);
    const primaryOptions = options({ tealstreetProxy: true, tealstreetTunnel: false });
    const primaryPlan = resolveEngineRoutingPlan('asterdex', primaryOptions, baseURL);
    expect(primaryPlan).not.toHaveProperty('readHttp');
    expect(primaryPlan).not.toHaveProperty('publicHeader');
    const primary = createEngineTransport('asterdex', primaryOptions, raw);
    await primary.request(privateRequest('GET'));
    expect(raw.request.mock.lastCall![0].url).toBe(`${PRIVATE_PROXY_ASIA}/${privateRequest('GET').url}`);
    routed.dispose!();
    primary.dispose!();
  });

  it.each([{ secondary_key: 'read-wallet' }, { secondary_secret: 'read-private-key' }])(
    'rejects partial secondary credentials %o',
    (partial) => {
      const opts = options(partial);
      expect(() => createEngineTransport('asterdex', opts, makeRaw())).toThrow('secondary signing keys');
      expect(() => resolveEngineRoutingPlan('asterdex', opts, baseURL)).toThrow('secondary signing keys');
    },
  );

  it('keeps direct transport direct when no secondary egress is configured', async () => {
    const raw = makeRaw();
    const opts = options({
      transport: directTransport,
      secondary_key: 'read-wallet',
      secondary_secret: 'read-private-key',
    });
    const routed = createEngineTransport('asterdex', opts, raw);
    await routed.request(privateRequest('GET'));
    expect(raw.request.mock.lastCall![0].url).toBe(privateRequest('GET').url);
    expect(mocks.token).not.toHaveBeenCalled();
    expect(resolveEngineRoutingPlan('asterdex', opts, baseURL).readHttp).toEqual(
      resolveEngineRoutingPlan('asterdex', opts, baseURL).http,
    );
    routed.dispose!();
  });
});

describe('typed public HTTP route hints', () => {
  it('keeps clock direct and CORS attempts out of account HOW routing and auth', async () => {
    const raw = makeRaw();
    const opts = options({
      tealstreetProxy: true,
      tealstreetTunnel: true,
      overrideProxyUrl: 'https://account.example/proxy',
      secondary_key: 'read-key',
      secondary_secret: 'read-secret',
    });
    const routed = createEngineTransport('asterdex', opts, raw);
    const url = 'https://fapi.asterdex.com/fapi/v3/time?exact=one%2Btwo';
    for (const [route, expected] of [
      ['direct', url],
      ['publicCors', `${PUBLIC_CORS_PROXY}/${url}`],
    ] as const) {
      await routed.request({ method: 'GET', route, url, headers: {} });
      const sent = raw.request.mock.lastCall![0];
      expect(sent.url).toBe(expected);
      expect(sent.headers).toEqual({});
      expect(sent).not.toHaveProperty('route');
    }
    expect(mocks.adapter).not.toHaveBeenCalled();
    expect(mocks.token).not.toHaveBeenCalled();
    await routed.request({ method: 'GET', url, headers: {} });
    expect(mocks.adapter).toHaveBeenCalledOnce();
    const plan = resolveEngineRoutingPlan('asterdex', opts, 'https://fapi.asterdex.com');
    expect(plan.publicCorsHttp).toEqual({
      mode: 'direct',
      prefix: PUBLIC_CORS_PROXY,
      authorize: false,
      authorizationHeader: 'Authorization',
      headers: {},
    });
    expect(JSON.stringify(plan)).not.toMatch(/read-key|read-secret|venue-key|venue-secret|proxy-jwt/);
    routed.dispose!();
  });

  it.each(['binance', 'hyperliquid'] as const)(
    'explicit public CORS is direct even for %s special account policies',
    async (exchange) => {
      const raw = makeRaw();
      const routed = createEngineTransport(exchange, options({ tealstreetTunnel: true }), raw);
      const url = 'https://venue.test/time';
      await routed.request({ method: 'GET', route: 'publicCors', url, headers: {} });
      expect(raw.request.mock.lastCall![0].url).toBe(`${PUBLIC_CORS_PROXY}/${url}`);
      expect(mocks.adapter).not.toHaveBeenCalled();
      expect(mocks.token).not.toHaveBeenCalled();
      routed.dispose!();
    },
  );

  it.each(['asterdex', 'txflow'] as const)('fails unconfigured CORS before network for %s and preserves ordinary direct requests', async (exchange) => {
    const raw = makeRaw();
    const routed = createEngineTransport(exchange, options({ transport: directTransport }), raw);
    const input = { method: 'GET', url: 'https://fapi.asterdex.com/fapi/v1/time', headers: {} };
    await expect(routed.request({ ...input, route: 'publicCors' })).rejects.toThrow('not configured');
    expect(raw.request).not.toHaveBeenCalled();
    await routed.request(input);
    expect(raw.request.mock.lastCall![0].url).toBe(input.url);
    expect(
      resolveEngineRoutingPlan(exchange, options({ transport: directTransport }), 'https://fapi.asterdex.com'),
    ).not.toHaveProperty('publicCorsHttp');
    routed.dispose!();
  });

  it('disposal aborts direct and CORS route streams using the shared deadline', async () => {
    vi.useFakeTimers();
    const raw = makeRaw();
    raw.request.mockResolvedValue({
      status: 200,
      body: (async function* () {
        yield new Uint8Array([1]);
      })(),
    });
    const routed = createEngineTransport('asterdex', options({ tealstreetTunnel: true }), raw);
    for (const route of ['direct', 'publicCors'] as const)
      await routed.request({ method: 'GET', route, url: 'https://venue.test/time', headers: {} });
    expect(vi.getTimerCount()).toBe(2);
    routed.dispose!();
    expect(vi.getTimerCount()).toBe(0);
    expect(raw.request.mock.calls.every(([request]) => request.signal?.aborted)).toBe(true);
  });
});

describe('typed HTTP operation deadlines', () => {
  it.each([
    [undefined, 20_000],
    [5_000, 5_000],
  ] as const)('bounds the whole request at %s with default 20 seconds', async (timeoutMs, deadline) => {
    vi.useFakeTimers();
    const schedule = vi.spyOn(globalThis, 'setTimeout');
    const cancel = vi.spyOn(globalThis, 'clearTimeout');
    const raw = makeRaw();
    raw.request.mockImplementation(() => new Promise(() => {}));
    const routed = createEngineTransport('asterdex', options({ transport: directTransport }), raw);
    const pending = routed.request({
      method: 'GET',
      route: 'direct',
      timeoutMs,
      url: 'https://venue.test/time',
      headers: {},
    });
    const deadlineTimers = schedule.mock.calls.flatMap(([, delay], index) =>
      delay === deadline ? [schedule.mock.results[index].value] : [],
    );
    expect(deadlineTimers).toHaveLength(1);
    const rejected = expect(pending).rejects.toThrow('timed out');
    await vi.advanceTimersByTimeAsync(deadline - 1);
    expect(raw.request.mock.lastCall![0].signal?.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await rejected;
    expect(raw.request.mock.lastCall![0]).not.toHaveProperty('timeoutMs');
    expect(raw.request.mock.lastCall![0]).not.toHaveProperty('route');
    routed.dispose!();
    // Other environment tasks may schedule timers during the asynchronous clock
    // advance. Verify cleanup of this request's deadline by its own handle.
    expect(cancel).toHaveBeenCalledWith(deadlineTimers[0]);
  });

  it('disposal clears its pending deadline while preserving an unrelated caller timer', async () => {
    vi.useFakeTimers();
    const caller = vi.fn();
    const callerTimer = setTimeout(caller, 60_000);
    const schedule = vi.spyOn(globalThis, 'setTimeout');
    const cancel = vi.spyOn(globalThis, 'clearTimeout');
    const raw = makeRaw();
    raw.request.mockImplementation(() => new Promise(() => {}));
    const routed = createEngineTransport('asterdex', options({ transport: directTransport }), raw);
    const pending = routed.request({ method: 'GET', route: 'direct', url: 'https://venue.test/time', headers: {} });
    const deadlineTimers = schedule.mock.calls.flatMap(([, delay], index) =>
      delay === 20_000 ? [schedule.mock.results[index].value] : [],
    );
    expect(deadlineTimers).toHaveLength(1);
    const rejected = expect(pending).rejects.toThrow('disposed');
    await vi.waitFor(() => expect(raw.request).toHaveBeenCalledTimes(1));
    routed.dispose!();
    await rejected;
    expect(raw.request.mock.lastCall![0].signal?.aborted).toBe(true);
    expect(cancel).toHaveBeenCalledWith(deadlineTimers[0]);
    expect(cancel).not.toHaveBeenCalledWith(callerTimer);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(caller).toHaveBeenCalledTimes(1);
    clearTimeout(callerTimer);
  });

  it('keeps the 5-second clock budget active through a stalled CORS response body', async () => {
    vi.useFakeTimers();
    const raw = makeRaw();
    raw.request.mockResolvedValue({
      status: 200,
      body: { [Symbol.asyncIterator]: () => ({ next: () => new Promise(() => {}) }) },
    });
    const routed = createEngineTransport('asterdex', options({ tealstreetTunnel: true }), raw);
    const response = await routed.request({
      method: 'GET',
      route: 'publicCors',
      timeoutMs: 5_000,
      url: 'https://venue.test/time',
      headers: {},
    });
    const body = response.body as AsyncIterable<Uint8Array>;
    const rejected = expect(body[Symbol.asyncIterator]().next()).rejects.toThrow('timed out');
    await vi.advanceTimersByTimeAsync(4_999);
    expect(raw.request.mock.lastCall![0].signal?.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await rejected;
    expect(raw.request.mock.lastCall![0].signal?.aborted).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
    routed.dispose!();
  });

  it('passes only the remaining budget to HOW without serializing timeout metadata', async () => {
    const routed = createEngineTransport('asterdex', options({ tealstreetTunnel: true }), makeRaw());
    await routed.request({ method: 'GET', timeoutMs: 5_000, url: 'https://venue.test/time', headers: {} });
    const config = mocks.adapter.mock.lastCall![0];
    expect(config.timeout).toBeGreaterThan(0);
    expect(config.timeout).toBeLessThanOrEqual(5_000);
    expect(config).not.toHaveProperty('timeoutMs');
    routed.dispose!();
  });
});

describe('secondary GET identity egress', () => {
  it('Kraken Spot matches the actual secondary public GET client while private POSTs retain primary signatures and egress', async () => {
    const primarySecret = Buffer.from('primary-spot-secret').toString('base64');
    const opts = options({
      key: 'primary-spot-key',
      secret: primarySecret,
      secondary_key: 'secondary-spot-key',
      secondary_secret: Buffer.from('secondary-spot-secret').toString('base64'),
      secondaryProxyUrl: 'https://secondary.example/spot',
      tealstreetProxy: true,
      tealstreetTunnel: false,
    });
    const parent = {
      name: 'krakenspot', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: vi.fn() },
    } as unknown as BaseExchange;
    const captured: Request[] = [];
    const [client] = createApiWithSecondaryKey(opts, parent, (selected, exchange) => {
      const api = createKrakenSpotAPI(selected, exchange);
      api.defaults.adapter = async (config) => {
        captured.push({
          method: config.method!.toUpperCase(),
          url: `${config.baseURL}${config.url}`,
          headers: config.headers.toJSON() as Record<string, string>,
          ...(config.data !== undefined ? { body: config.data as string } : {}),
        });
        return { status: 200, statusText: 'OK', config, headers: {}, data: { error: [], result: {} } };
      };
      return api;
    });
    await client.get('/0/public/AssetPairs');
    await client.get('/0/public/Ticker');
    await client.post('/0/private/Balance', {});
    await client.post('/0/private/GetWebSocketsToken', {});
    expect(captured.map(({ url }) => url)).toEqual([
      'https://secondary.example/spot/https://api.kraken.com/0/public/AssetPairs',
      'https://secondary.example/spot/https://api.kraken.com/0/public/Ticker',
      `${PRIVATE_PROXY_ASIA}/https://api.kraken.com/0/private/Balance`,
      `${PRIVATE_PROXY_ASIA}/https://api.kraken.com/0/private/GetWebSocketsToken`,
    ]);
    for (const input of captured.slice(0, 2)) {
      expect(input.headers['API-Key']).toBeUndefined();
      expect(input.headers['API-Sign']).toBeUndefined();
    }
    const raw = makeRaw();
    const routed = createEngineTransport('krakenspot', opts, raw);
    try {
      for (const input of captured) {
        const venueUrl = `https://api.kraken.com${input.url.split('https://api.kraken.com')[1]}`;
        if (input.method === 'POST') {
          expect(input.headers['API-Key']).toBe('primary-spot-key');
          const nonce = new URLSearchParams(input.body).get('nonce');
          const digest = createHash('sha256').update(nonce + input.body!).digest();
          const signature = createHmac('sha512', Buffer.from(primarySecret, 'base64'))
            .update(new URL(venueUrl).pathname).update(digest).digest('base64');
          expect(input.headers['API-Sign']).toBe(signature);
        }
        await routed.request({ ...input, url: venueUrl });
        const delivered = raw.request.mock.lastCall![0];
        expect(delivered.url).toBe(input.url);
        expect(delivered.body).toBe(input.body);
        expect(delivered.headers['API-Key']).toBe(input.headers['API-Key']);
        expect(delivered.headers['API-Sign']).toBe(input.headers['API-Sign']);
      }
      const plan = resolveEngineRoutingPlan('krakenspot', opts, 'https://api.kraken.com');
      expect(plan.readHeader).toBe('API-Key');
      expect(plan.readPublic).toBe(true);
      expect(plan.readHttp?.prefix).toBe('https://secondary.example/spot');
      expect(plan.http.prefix).toBe(PRIVATE_PROXY_ASIA);
      for (const secret of [opts.key, opts.secondary_key, opts.secret, opts.secondary_secret]) {
        expect(JSON.stringify(plan)).not.toContain(secret);
      }
    } finally {
      routed.dispose!();
    }
  });
  const venues: Array<[ExchangeName, string]> = [
    ['binance', 'X-MBX-APIKEY'],
    ['binancepm', 'X-MBX-APIKEY'],
    ['binancespot', 'X-MBX-APIKEY'],
    ['binanceinverse', 'X-MBX-APIKEY'],
    ['bybit', 'X-BAPI-API-KEY'],
    ['bybitinverse', 'X-BAPI-API-KEY'],
    ['bybitspot', 'X-BAPI-API-KEY'],
    ['bybitdemo', 'X-BAPI-API-KEY'],
    ['okx', 'OK-ACCESS-KEY'],
    ['okxeu', 'OK-ACCESS-KEY'],
    ['okxdemo', 'OK-ACCESS-KEY'],
    ['okxeudemo', 'OK-ACCESS-KEY'],
    ['phemex', 'X-PHEMEX-ACCESS-TOKEN'],
    ['backpack', 'X-API-Key'],
    ['kraken', 'APIKey'],
    ['krakenspot', 'API-Key'],
    ['bitget', 'ACCESS-KEY'],
    ['bitgetusdc', 'ACCESS-KEY'],
    ['bitgetinverse', 'ACCESS-KEY'],
    ['kucoin', 'KC-API-KEY'],
    ['kucoinspot', 'KC-API-KEY'],
    ['mexc', 'ApiKey'],
    ['bitunix', 'api-key'],
    ['bingx', 'X-BX-APIKEY'],
    ['bitrue', 'X-CH-APIKEY'],
    ['weex', 'ACCESS-KEY'],
    ['blofin', 'ACCESS-KEY'],
    ['woo', 'x-api-key'],
  ];
  it.each(venues)(
    '%s routes GET through the secondary client while primary exceptions and mutations keep account egress',
    async (venue, header) => {
      const opts = options({
        passphrase: 'primary-pass',
        tealstreetProxy: true,
        tealstreetTunnel: false,
        secondary_key: 'venue-key',
        secondary_secret: 'read-secret',
        secondary_passphrase: 'read-pass',
        secondaryProxyUrl: 'https://secondary.example/egress',
      });
      const raw = makeRaw();
      const routed = createEngineTransport(venue, opts, raw);
      // Both KuCoin factories install HOW for private clients even with an explicit false flag.
      const viaHow = venue === 'kucoin' || venue === 'kucoinspot';
      const delivered = () => viaHow
        ? { ...mocks.adapter.mock.lastCall![0], headers: mocks.adapter.mock.lastCall![0].headers.toJSON() }
        : raw.request.mock.lastCall![0];
      const input: Request = {
        method: 'GET',
        url: 'https://venue.example/private?signature=exact',
        headers: { [header.toLowerCase()]: 'venue-key' },
      };
      await routed.request(input);
      expect(delivered().url).toBe(`https://secondary.example/egress/${input.url}`);
      expect(delivered().headers[header.toLowerCase()]).toBe('venue-key');
      await routed.request({ ...input, route: 'primary' });
      const primaryUrl = delivered().url;
      expect(primaryUrl).not.toContain('secondary.example');
      for (const method of ['POST', 'PUT', 'PATCH', 'DELETE']) {
        await routed.request({ ...input, method });
        expect(delivered().url).toBe(primaryUrl);
      }
      // A declared read-key request uses the read egress whatever its method.
      await routed.request({ ...input, method: 'POST', route: 'secondary' });
      expect(delivered().url).toBe(`https://secondary.example/egress/${input.url}`);
      await routed.request({ ...input, headers: {} });
      if (['binance', 'binancepm', 'binanceinverse'].includes(venue))
        expect(delivered().url).not.toContain('secondary.example');
      else expect(delivered().url).toBe(`https://secondary.example/egress/${input.url}`);
      expect(delivered().headers[header.toLowerCase()]).toBeUndefined();
      const plan = resolveEngineRoutingPlan(venue, opts, 'https://venue.example');
      expect(plan.readHeader).toBe(header);
      expect(plan.readPublic).toBe(!['binance', 'binancepm', 'binanceinverse'].includes(venue));
      expect(plan.readHttp?.prefix).toBe('https://secondary.example/egress');
      expect(JSON.stringify({ readHeader: plan.readHeader, readHttp: plan.readHttp })).not.toMatch(
        /read-secret|read-pass|venue-key/,
      );
      routed.dispose!();
    },
  );
  it.each(['phemex', 'weex', 'kucoin', 'kucoinspot', 'okx', 'backpack', 'krakenspot', 'bitunix'] as ExchangeName[])(
    '%s rejects partial read identities before any transport effect',
    (venue) => {
      for (const extra of [
        { secondary_key: '' },
        { secondary_key: 'read' },
        { secondary_secret: 'read' },
        { secondary_passphrase: 'read' },
      ]) {
        const raw = makeRaw();
        expect(() => createEngineTransport(venue, options(extra), raw)).toThrow('secondary signing keys');
        expect(raw.request).not.toHaveBeenCalled();
        expect(raw.connect).not.toHaveBeenCalled();
      }
    },
  );
  it.each(['bybit', 'phemex', 'kucoin', 'kucoinspot', 'woo', 'bitunix'] as ExchangeName[])(
    '%s secondary GET retains HOW encapsulation and primary-only request bytes',
    async (venue) => {
      const headers: Partial<Record<ExchangeName, string>> = {
        bybit: 'X-BAPI-API-KEY',
        phemex: 'X-PHEMEX-ACCESS-TOKEN',
        kucoin: 'KC-API-KEY',
        kucoinspot: 'KC-API-KEY',
        woo: 'x-api-key',
        bitunix: 'api-key',
      };
      const opts = options({
        passphrase: 'primary-pass',
        tealstreetTunnel: true,
        secondary_key: 'read-key',
        secondary_secret: 'read-secret',
        secondaryProxyUrl: 'https://secondary.example/egress',
      });
      const raw = makeRaw();
      const routed = createEngineTransport(venue, opts, raw);
      const input: Request = {
        method: 'GET',
        url: 'https://venue.example/account?signature=exact',
        headers: { [headers[venue]!]: 'read-key' },
      };
      await routed.request(input);
      expect(mocks.adapter.mock.lastCall![0].url).toBe(`https://secondary.example/egress/${input.url}`);
      await routed.request({ ...input, route: 'primary' });
      expect(mocks.adapter.mock.lastCall![0].url).not.toContain('secondary.example');
      expect(raw.request).not.toHaveBeenCalled();
      const plan = resolveEngineRoutingPlan(venue, opts, 'https://venue.example');
      expect(plan.readHttp?.mode).toBe('how');
      expect(plan.readHttp?.tunnelUrl).toBe(plan.http.tunnelUrl);
      routed.dispose!();
    },
  );
  it('does not adopt secondary credentials for an unsupported venue', () => {
    expect(() =>
      createEngineTransport('hyperliquid', options({ secondary_key: 'read', secondary_secret: 'secret' }), makeRaw()),
    ).toThrow('unsupported');
  });
});

describe('Bybit product public sockets', () => {
  const brazil = options({ bybitSiteId: 'BRA_BTL', tealstreetTunnel: false });

  it('routes inverse and spot public streams to their own hosts and options direct', () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', brazil, raw);
    const opened = (url: string) => {
      routed.connect(url, handlers('public'));
      return raw.connect.mock.lastCall?.[0];
    };
    expect(opened('wss://stream.bybit.com/v5/public/linear')).toBe(resolveExchangeRouting('bybit', brazil).wsPublic.host);
    expect(opened('wss://stream.bybit.com/v5/public/inverse')).toBe(
      resolveExchangeRouting('bybitinverse', brazil).wsPublic.host,
    );
    expect(opened('wss://stream.bybit.com/v5/public/spot')).toBe(resolveExchangeRouting('bybitspot', brazil).wsPublic.host);
    // No option proxy is deployed; the native URL beats the linear host.
    expect(opened('wss://stream.bybit.com/v5/public/option')).toBe('wss://stream.bybit.com/v5/public/option');
    // Private streams keep the private route.
    routed.connect('wss://stream.bybit.com/v5/private', handlers('private'));
    expect(raw.connect.mock.lastCall?.[0]).toBe(resolveExchangeRouting('bybit', brazil).wsPrivate.host);
  });

  it('serializes the same product routes for native hosts', () => {
    const plan = resolveEngineRoutingPlan('bybit', brazil, 'https://api.bybit.com');
    expect(plan.publicWsRoutes).toEqual([
      { contains: '/v5/public/inverse', route: expect.objectContaining({ host: resolveExchangeRouting('bybitinverse', brazil).wsPublic.host }) },
      { contains: '/v5/public/spot', route: expect.objectContaining({ host: resolveExchangeRouting('bybitspot', brazil).wsPublic.host }) },
      { contains: '/v5/public/option', route: {} },
    ]);
    expect(resolveEngineRoutingPlan('bybitinverse', brazil, 'https://api.bybit.com').publicWsRoutes).toBeUndefined();
  });
});


describe('Bitunix explicit WebSocket routing', () => {
  it.each([true, false, undefined])('honors explicit route=%s from the first connect for every stable feed', (tealstreetWsRoute) => {
    const opts = options({ tealstreetWsRoute });
    const plan = resolveEngineRoutingPlan('bitunix', opts, 'https://fapi.bitunix.com');
    // Legacy TS callers keep consuming fallback from the shared resolver.
    const legacy = resolveExchangeRouting('bitunix', opts);
    expect(legacy.wsPublic.host).toBeUndefined();
    expect(legacy.wsPublic.fallback).toBe('wss://bitunix-fapi-proxy-ja.tealstreet.io/public/');
    const feeds = [
      ['public', 'ticker:0', 'wss://fapi.bitunix.com/public/', 'wss://bitunix-fapi-proxy-ja.tealstreet.io/public/'],
      ['public', 'price:300', 'wss://fapi.bitunix.com/public/', 'wss://bitunix-fapi-proxy-ja.tealstreet.io/public/'],
      ['private', 'private', 'wss://fapi.bitunix.com/private/', 'wss://bitunix-fapi-proxy-ja.tealstreet.io/private/'],
      ['business', 'futures', 'wss://api.bitunix.com/ws-futures/?from=contract&compress=2', 'wss://bitunix-api-proxy-ja.tealstreet.io/ws-futures/?from=contract&compress=2'],
    ] as const;
    const raw = makeRaw();
    const transport = createEngineTransport('bitunix', opts, raw);
    try {
      for (const [channel, feed, url, proxy] of feeds) {
        expect(plan.ws[channel].host).toBe(tealstreetWsRoute ? proxy : undefined);
        expect(plan.ws[channel].fallback).toBe(tealstreetWsRoute ? undefined : proxy);
        for (let attempt = 0; attempt < 3; attempt++) {
          transport.connect(url, { channel, context: { feed, recover: false }, onOpen() {}, onMessage() {}, onClose() {} });
          expect(raw.connect.mock.lastCall![0]).toBe(tealstreetWsRoute || attempt === 2 ? proxy : url);
        }
        transport.connect(url, { channel, context: { feed: `recover:${feed}`, recover: true }, onOpen() {}, onMessage() {}, onClose() {} });
        expect(raw.connect.mock.lastCall![0]).toBe(proxy);
      }
    } finally { transport.dispose!(); }
  });

  it('keeps direct-provider sockets direct and reports unsupported explicit routing', () => {
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const raw = makeRaw();
    const transport = createEngineTransport('bitunix', options({ tealstreetWsRoute: true, transport: directTransport }), raw);
    try {
      const url = 'wss://fapi.bitunix.com/private/';
      transport.connect(url, handlers('private'));
      expect(raw.connect.mock.lastCall![0]).toBe(url);
      expect(warning).toHaveBeenCalledWith(expect.stringContaining('[bitunix] tealstreetWsRoute requested but the direct transport is active'));
    } finally { transport.dispose!(); warning.mockRestore(); }
  });

  it('does not force Bitget fallback-only channels with the Bitunix option', () => {
    const plan = resolveEngineRoutingPlan('bitget', options({ tealstreetWsRoute: true }), 'https://api.bitget.com');
    expect(plan.ws.public.host).toBeUndefined();
    expect(plan.ws.private.host).toBeUndefined();
  });
});


it('routes provider-owned secondary reads using metadata without credentials in transport options', async () => {
  const raw = makeRaw();
  const safe = { transport: tealstreetTransport, tealstreetTunnel: false, secondaryProxyUrl: 'https://read.invalid' } as ExchangeOptions;
  const routed = createEngineTransport('binance', safe, raw, { hasSecondary: true });
  const url = 'https://fapi.binance.com/fapi/v1/account';
  try {
    await routed.request({ method: 'GET', url, headers: { 'X-MBX-APIKEY': 'read-key' } });
    expect(raw.request.mock.lastCall![0].url).toBe(`https://read.invalid/${url}`);
    await routed.request({ method: 'GET', route: 'primary', url, headers: { 'X-MBX-APIKEY': 'venue-key' } });
    expect(raw.request.mock.lastCall![0].url).not.toContain('read.invalid');
    expect(safe).not.toHaveProperty('key');
    expect(safe).not.toHaveProperty('secret');
    expect(safe).not.toHaveProperty('secondary_key');
  } finally { routed.dispose?.(); }
});

describe('managed engine HTTP preparation', () => {
  const managedRaw = () => {
    const raw = makeRaw();
    const cancel = vi.fn();
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array([49]) }));
    const ready = vi.fn(() => true);
    const bind = vi.fn((_packet: Request) => ({ ready, dispatch }));
    const prepareHttp = vi.fn<NonNullable<Transport['prepareHttp']>>(async () => ({ bind, cancel }));
    return { raw: { ...raw, prepareHttp }, cancel, dispatch, ready, bind, prepareHttp };
  };
  const packet = () => ({ ...request(), request: 77, headers: {}, intent: 'primary' as const });

  it('finishes proxy authorization before readiness and binds the exact signed packet without I/O', async () => {
    const probe = managedRaw();
    let token!: (value: string) => void;
    mocks.token.mockReturnValue(
      new Promise<string>((resolve) => {
        token = resolve;
      }),
    );
    const routed = createEngineTransport('bybit', options({ tealstreetProxy: true }), probe.raw);
    const original = packet();
    const preparing = routed.prepareHttp!(original);
    expect(probe.prepareHttp).not.toHaveBeenCalled();
    expect(probe.raw.request).not.toHaveBeenCalled();
    token('managed-proxy');
    const prepared = await preparing;
    expect(probe.prepareHttp.mock.lastCall![0].headers.Authorization).toBe('Bearer managed-proxy');
    expect(probe.dispatch).not.toHaveBeenCalled();
    const signed = {
      ...original,
      url: original.url + '&stamp=123.000&sig=%2B+%20',
      headers: { 'X-BAPI-SIGN': 'exact-signature' },
      body: ' {"id":900719925474099312345,"p":1.2300}\n',
    };
    const bound = prepared.bind(signed);
    const final = probe.bind.mock.lastCall![0] as Request;
    expect(final.url).toBe(`${PRIVATE_PROXY_ASIA}/${signed.url}`);
    expect(final.body).toBe(signed.body);
    expect(final.headers).toMatchObject({ 'X-BAPI-SIGN': 'exact-signature', Authorization: 'Bearer managed-proxy' });
    expect(bound.ready()).toBe(true);
    expect(probe.dispatch).not.toHaveBeenCalled();
    const response = bound.dispatch();
    expect(probe.dispatch).toHaveBeenCalledTimes(1);
    expect(probe.raw.request).not.toHaveBeenCalled();
    await response;
    expect(() => bound.dispatch()).toThrow();
    routed.dispose!();
  });

  it('routes a POST read by secondary intent before its key header exists', async () => {
    const probe = managedRaw();
    const routed = createEngineTransport(
      'binance',
      options({ secondary_key: 'read-key', secondary_secret: 'read-secret', secondaryProxyUrl: 'https://read.proxy' }),
      probe.raw,
    );
    const original = { ...packet(), url: 'https://fapi.binance.com/private/read', intent: 'secondary' as const };
    const prepared = await routed.prepareHttp!(original);
    expect(probe.prepareHttp.mock.lastCall![0].url).toBe(`https://read.proxy/${original.url}`);
    const signed = { ...original, headers: { 'X-MBX-APIKEY': 'read-key' } };
    await prepared.bind(signed).dispatch();
    expect((probe.bind.mock.lastCall![0] as Request).headers['X-MBX-APIKEY']).toBe('read-key');
    expect(probe.raw.request).not.toHaveBeenCalled();
    routed.dispose!();
  });

  it('routes public Binance futures by intent even if input carries a misleading key header', async () => {
    const probe = managedRaw();
    const routed = createEngineTransport('binance', options({ tealstreetProxy: true }), probe.raw);
    const original = {
      ...packet(),
      url: 'https://fapi.binance.com/public',
      intent: 'public' as const,
      headers: { 'X-MBX-APIKEY': 'not-a-route-selector' },
    };
    const prepared = await routed.prepareHttp!(original);
    expect(probe.prepareHttp.mock.lastCall![0].url).toBe(original.url);
    expect(mocks.token).not.toHaveBeenCalled();
    prepared.cancel!();
    routed.dispose!();
  });

  it('fails closed when raw or tunnel adapters cannot prepare, without invoking their opaque send', async () => {
    for (const tealstreetTunnel of [false, true]) {
      const raw = makeRaw();
      const routed = createEngineTransport('bybit', options({ tealstreetTunnel }), raw);
      await expect(routed.prepareHttp!(packet())).rejects.toMatchObject({ code: 'HTTP_PREPARATION_UNSUPPORTED' });
      expect(raw.request).not.toHaveBeenCalled();
      expect(mocks.adapter).not.toHaveBeenCalled();
      routed.dispose!();
    }
  });

  it('does not renew the original deadline when binding after readiness', async () => {
    vi.useFakeTimers();
    const probe = managedRaw();
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    const original = { ...packet(), timeoutMs: 100 };
    const prepared = await routed.prepareHttp!(original);
    await vi.advanceTimersByTimeAsync(75);
    const bound = prepared.bind(original);
    expect((probe.bind.mock.lastCall![0] as Request).timeoutMs).toBe(25);
    await vi.advanceTimersByTimeAsync(25);
    expect(() => bound.ready()).toThrow('timed out');
    expect(probe.cancel).toHaveBeenCalledTimes(1);
    expect(probe.dispatch).not.toHaveBeenCalled();
    routed.dispose!();
  });

  it.each(['method', 'origin', 'route', 'request'] as const)(
    'rejects changed %s before binding the transport',
    async (field) => {
      const probe = managedRaw();
      const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
      const original = packet();
      const prepared = await routed.prepareHttp!(original);
      const changed = {
        ...original,
        ...{
          method: { method: 'GET' },
          origin: { url: 'https://other.test/read' },
          route: { route: 'direct' as const },
          request: { request: 78 },
        }[field],
      };
      expect(() => prepared.bind(changed)).toThrow();
      expect(probe.bind).not.toHaveBeenCalled();
      expect(probe.cancel).toHaveBeenCalledTimes(1);
      routed.dispose!();
    },
  );

  it('cancels late readiness after disposal without disturbing a separate request', async () => {
    const probe = managedRaw();
    let resolve!: (prepared: { bind: typeof probe.bind; cancel: typeof probe.cancel }) => void;
    probe.prepareHttp.mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    const first = routed.prepareHttp!(packet());
    const rejected = expect(first).rejects.toThrow('disposed');
    routed.dispose!();
    await rejected;
    resolve({ bind: probe.bind, cancel: probe.cancel });
    await Promise.resolve();
    expect(probe.cancel).toHaveBeenCalledTimes(1);
    expect(probe.dispatch).not.toHaveBeenCalled();
    const peer = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    await (await peer.prepareHttp!(packet())).bind(packet()).dispatch();
    expect(probe.dispatch).toHaveBeenCalledTimes(1);
    peer.dispose!();
  });

  it.each(['x-site-id', 'X-SITE-ID'])(
    'rejects a conflicting signed site header %s before inner bind',
    async (header) => {
      const probe = managedRaw();
      const routed = createEngineTransport(
        'bybit',
        options({ transport: directTransport, bybitSiteId: 'BRA_BTL' }),
        probe.raw,
      );
      const original = packet();
      const prepared = await routed.prepareHttp!(original);
      expect(() => prepared.bind({ ...original, headers: { [header]: 'other-site' } })).toThrow();
      expect(probe.bind).not.toHaveBeenCalled();
      expect(probe.dispatch).not.toHaveBeenCalled();
      routed.dispose!();
    },
  );

  it('releases a bound resource exactly once without cancelling a shared peer', async () => {
    const probe = managedRaw();
    const cancelBound = vi.fn();
    probe.bind.mockReturnValueOnce({ ready: probe.ready, dispatch: probe.dispatch, cancel: cancelBound } as ReturnType<
      typeof probe.bind
    >);
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    const first = (await routed.prepareHttp!(packet())).bind(packet());
    const peer = (await routed.prepareHttp!({ ...packet(), request: 78 })).bind({ ...packet(), request: 78 });
    first.cancel!();
    first.cancel!();
    expect(cancelBound).toHaveBeenCalledTimes(1);
    expect(probe.cancel).toHaveBeenCalledTimes(1);
    expect(peer.ready()).toBe(true);
    await peer.dispatch();
    expect(probe.dispatch).toHaveBeenCalledTimes(1);
    routed.dispose!();
  });

  it.each(['missing', 'null', 'no-bind', 'missing-bound', 'no-ready', 'no-dispatch'])(
    'rejects malformed managed %s resources without legacy send',
    async (shape) => {
      const probe = managedRaw();
      const invalid: Record<string, unknown> = {
        missing: undefined,
        null: null,
        'no-bind': { cancel: probe.cancel },
        'missing-bound': { bind: () => undefined },
        'no-ready': { bind: () => ({ dispatch: probe.dispatch }) },
        'no-dispatch': { bind: () => ({ ready: probe.ready }) },
      };
      probe.prepareHttp.mockResolvedValueOnce(invalid[shape] as never);
      const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
      try {
        await expect(
          (async () => {
            const prepared = await routed.prepareHttp!(packet());
            await prepared.bind(packet()).dispatch();
          })(),
        ).rejects.toMatchObject({ code: 'HTTP_PREPARATION_INVALID' });
        expect(probe.raw.request).not.toHaveBeenCalled();
        expect(probe.dispatch).not.toHaveBeenCalled();
      } finally {
        routed.dispose!();
      }
    },
  );

  it('rejects legacy signed-header mutation inside the provider route callback', async () => {
    const raw = makeRaw();
    const provider = {
      ...directTransport,
      rest: vi.fn((context: Parameters<typeof directTransport.rest>[0]) => {
        (context.config.headers as Record<string, string>).Authorization = 'mutated';
        return { baseURL: context.config.baseURL, proxied: false };
      }),
    };
    const routed = createEngineTransport('coinbase', options({ transport: provider }), raw);
    await expect(
      routed.request({
        ...request('https://api.coinbase.com/api/v3/brokerage/orders'),
        headers: { Authorization: 'signed-jwt' },
      }),
    ).rejects.toThrow('changed signed');
    expect(raw.request).not.toHaveBeenCalled();
    routed.dispose!();
  });

  it.each(['bytes', 'stream'])('releases inner resources once after a successful %s response and explicit cancellation', async (mode) => {
    const probe = managedRaw();
    const cancelBound = vi.fn();
    const dispatch = vi.fn(async () => ({ status: 200, body: mode === 'bytes' ? new Uint8Array([1]) :
      (async function* () { yield new Uint8Array([1]); })() }));
    probe.bind.mockReturnValueOnce({ ready: probe.ready, dispatch, cancel: cancelBound } as ReturnType<typeof probe.bind>);
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    const prepared = await routed.prepareHttp!(packet());
    const bound = prepared.bind(packet());
    const response = await bound.dispatch();
    if (!(response.body instanceof Uint8Array)) for await (const chunk of response.body) expect(chunk).toEqual(new Uint8Array([1]));
    prepared.cancel!();
    bound.cancel!();
    expect(cancelBound).toHaveBeenCalledTimes(1);
    expect(probe.cancel).toHaveBeenCalledTimes(1);
    routed.dispose!();
    expect(cancelBound).toHaveBeenCalledTimes(1);
  });

  it('retains legacy configured site-header precedence', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('bybit', options({ transport: directTransport, bybitSiteId: 'BRA_BTL' }), raw);
    await routed.request({ ...request(), headers: { 'x-site-id': 'caller-value' } });
    expect(raw.request.mock.lastCall![0].headers['x-site-id']).toBe('BRA_BTL');
    routed.dispose!();
  });

  it('retires a bound resource if caller cancellation happens synchronously during bind', async () => {
    const probe = managedRaw();
    const controller = new AbortController();
    const cancelBound = vi.fn();
    probe.bind.mockImplementationOnce(() => {
      controller.abort(new Error('caller cancelled'));
      return { ready: probe.ready, dispatch: probe.dispatch, cancel: cancelBound } as ReturnType<typeof probe.bind>;
    });
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    const original = { ...packet(), signal: controller.signal };
    const prepared = await routed.prepareHttp!(original);
    expect(() => prepared.bind(original)).toThrow('caller cancelled');
    expect(cancelBound).toHaveBeenCalledTimes(1);
    expect(probe.cancel).toHaveBeenCalledTimes(1);
    expect(probe.dispatch).not.toHaveBeenCalled();
    routed.dispose!();
  });

  it('exposes the underlying prepared resource expiry in the engine raw clock domain', async () => {
    const probe = managedRaw();
    probe.prepareHttp.mockResolvedValueOnce({ bind: probe.bind, cancel: probe.cancel, expiresAt: 42 });
    const routed = createEngineTransport('bybit', options({ transport: directTransport }), probe.raw);
    const prepared = await routed.prepareHttp!(packet());
    expect(prepared.expiresAt).toBe(42);
    prepared.cancel!();
    routed.dispose!();
  });
});
