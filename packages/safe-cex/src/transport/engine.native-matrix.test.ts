import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import { directTransport } from './direct';
import { createEngineTransport, resolveEngineRoutingPlan } from './engine';
import { resolveExchangeRouting } from './tealstreet/routing/resolveExchangeRouting';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { setWsRelayEnabled } from './tealstreet/wsRelay';

const mocks = vi.hoisted(() => ({ adapter: vi.fn(), factory: vi.fn() }));
vi.mock('../accessToken', () => ({ getProxyToken: async () => 'fixture-proxy-token' }));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => true }));
vi.mock('../httpOverWebsocket/axiosHttpOverWebsocketAdapter', () => ({
  axiosHttpOverWebsocketAdapter: mocks.adapter,
  bigIntAxiosHttpOverWebsocketAdapter: mocks.adapter,
  createAxiosHttpOverWebsocketAdapter: mocks.factory,
}));

interface Case {
  name: string;
  exchange: ExchangeName;
  baseURL: string;
  options: Partial<ExchangeOptions>;
  direct?: boolean;
  how?: boolean;
  relay?: boolean;
}

const cases: Case[] = [
  { name: 'direct-provider', exchange: 'bybit', baseURL: 'https://api.bybit.com', options: {}, direct: true },
  { name: 'tealstreet-direct', exchange: 'bybit', baseURL: 'https://api.bybit.com', options: {} },
  { name: 'asia-pool', exchange: 'bybit', baseURL: 'https://api.bybit.com', options: { tealstreetProxy: true } },
  {
    name: 'region-ja-pool',
    exchange: 'bybit',
    baseURL: 'https://api.bybit.com',
    options: { tealstreetProxy: true, proxyRegion: 'JA' },
  },
  { name: 'jp-pool', exchange: 'okx', baseURL: 'https://www.okx.com', options: { tealstreetProxy: true } },
  {
    name: 'tiny-pool',
    exchange: 'grvt',
    baseURL: 'https://trades.grvt.io',
    options: { tealstreetProxy: true, proxyRegion: 'JA' },
  },
  {
    name: 'grvt-forced-proxy',
    exchange: 'grvt',
    baseURL: 'https://trades.grvt.io',
    options: { tealstreetTunnel: true },
  },
  {
    name: 'blofin-forced-proxy',
    exchange: 'blofin',
    baseURL: 'https://openapi.blofin.com',
    options: { tealstreetTunnel: true },
  },
  {
    name: 'override-direct',
    exchange: 'bybit',
    baseURL: 'https://api.bybit.com',
    options: { overrideProxyUrl: 'https://override.invalid/route' },
  },
  {
    name: 'override-proxy',
    exchange: 'bybit',
    baseURL: 'https://api.bybit.com',
    options: { tealstreetProxy: true, overrideProxyUrl: 'https://override.invalid/route' },
  },
  {
    name: 'how-ja',
    exchange: 'bybit',
    baseURL: 'https://api.bybit.com',
    options: { tealstreetTunnel: true, proxyRegion: 'JA' },
    how: true,
  },
  {
    name: 'how-sg',
    exchange: 'bybit',
    baseURL: 'https://api.bybit.com',
    options: { tealstreetTunnel: true, proxyRegion: 'SG' },
    how: true,
  },
  {
    name: 'how-override',
    exchange: 'bybit',
    baseURL: 'https://api.bybit.com',
    options: { tealstreetTunnel: true, overrideProxyUrl: 'https://override.invalid/route' },
    how: true,
  },
  { name: 'bingx-default-how', exchange: 'bingx', baseURL: 'https://open-api.bingx.com', options: {}, how: true },
  {
    name: 'bingx-explicit-direct',
    exchange: 'bingx',
    baseURL: 'https://open-api.bingx.com',
    options: { tealstreetTunnel: false },
  },
  {
    name: 'bingx-public-cors',
    exchange: 'bingx',
    baseURL: 'https://open-api.bingx.com',
    options: { publicDataOnly: true, tealstreetProxy: true, tealstreetTunnel: false },
  },
  {
    name: 'binance-public-private',
    exchange: 'binance',
    baseURL: 'https://fapi.binance.com',
    options: { tealstreetProxy: true, tealstreetTunnel: false },
  },
  {
    name: 'binance-how-public-private',
    exchange: 'binance',
    baseURL: 'https://fapi.binance.com',
    options: { tealstreetProxy: true, tealstreetTunnel: true },
    how: true,
  },
  { name: 'bybit-brazil', exchange: 'bybit', baseURL: 'https://api.bybit.com', options: { bybitSiteId: 'BRA_BTL' } },
  { name: 'okx-eea', exchange: 'okxeu', baseURL: 'https://eea.okx.com', options: { tealstreetProxy: true } },
  { name: 'okx-eea-demo', exchange: 'okxeudemo', baseURL: 'https://eea.okx.com', options: { tealstreetProxy: true } },
  { name: 'grvt-always-ws-proxy', exchange: 'grvt', baseURL: 'https://trades.grvt.io', options: {} },
  {
    name: 'lighter-ws-route',
    exchange: 'lighter',
    baseURL: 'https://mainnet.zklighter.elliot.ai',
    options: { tealstreetWsRoute: true },
  },
  { name: 'relay-enabled', exchange: 'okx', baseURL: 'https://www.okx.com', options: {}, relay: true },
  {
    name: 'weex-no-how',
    exchange: 'weex',
    baseURL: 'https://api-contract.weex.com',
    options: { tealstreetProxy: true, tealstreetTunnel: false },
  },
];

const fixturePath = resolve(__dirname, '../../../../sdk-rs/hosts/native/tests/fixtures/routing-plans.json');

const buildCase = async (input: Case) => {
  setWsRelayEnabled(Boolean(input.relay));
  const provider = input.direct ? directTransport : tealstreetTransport;
  const options: ExchangeOptions = {
    key: 'fixture-venue-key',
    secret: 'fixture-venue-secret',
    ...input.options,
    transport: provider,
  };
  const plan = resolveEngineRoutingPlan(input.exchange, options, input.baseURL);
  const routing = input.direct
    ? provider.wsRouting(input.exchange, options)
    : resolveExchangeRouting(input.exchange, options);
  const siteHeaders = input.options.bybitSiteId === 'BRA_BTL' ? { 'x-site-id': 'BRA_BTL' } : {};
  const http = [];
  const keyModes = input.exchange.startsWith('binance') ? [false, true] : [false];
  for (const keyed of keyModes) {
    const url = `${input.baseURL}/fixture?signature=a%2Fb`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...(keyed ? { 'x-mbx-apikey': 'fixture-venue-key' } : {}),
    };
    const publicBinance = input.exchange.startsWith('binance') && !keyed;
    // Reproduce the TS public-client bypass; other requests go through the real
    // provider independently, without passing the engine's pre-resolved plan.
    const route = publicBinance
      ? { baseURL: url, proxied: false }
      : provider.rest({ exchange: input.exchange, options, config: { baseURL: url, headers } });
    const target = route.baseURL ?? url;
    const prefix = target === url ? '' : target.slice(0, -url.length - 1);
    const selected = publicBinance ? plan.publicHttp! : plan.http;
    expect(selected.prefix, input.name).toBe(prefix);
    expect(selected.authorize, input.name).toBe(route.proxied);
    expect(selected.mode, input.name).toBe(input.how ? 'how' : 'direct');
    const rawRequest = vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array() });
    const raw: Transport = {
      now: () => 0,
      setTimeout: () => 0,
      clearTimeout: () => {},
      request: rawRequest,
      connect: () => ({ send() {}, close() {} }),
    };
    await createEngineTransport(input.exchange, options, raw).request({ method: 'GET', url, headers });
    const delivered = input.how ? mocks.adapter.mock.lastCall![0] : rawRequest.mock.lastCall![0];
    const deliveredHeaders = input.how ? delivered.headers.toJSON() : delivered.headers;
    expect(delivered.url).toBe(target);
    const expectedHeaders: Record<string, string> = { ...headers, ...selected.headers };
    if (route.proxied) expectedHeaders[selected.authorizationHeader] = 'Bearer fixture-proxy-token';
    expect(deliveredHeaders).toEqual(expectedHeaders);
    http.push({
      request: { url, headers },
      expected: {
        url: target,
        prefix,
        mode: selected.mode,
        authorize: route.proxied,
        authorizationHeader: selected.authorizationHeader,
        headers: selected.headers,
        tunnelUrl: selected.tunnelUrl ?? null,
      },
    });
  }
  const sockets = (['public', 'private', 'business'] as const).map((channel) => {
    const route = routing[channel === 'public' ? 'wsPublic' : channel === 'private' ? 'wsPrivate' : 'wsBusiness'];
    const url = `wss://venue.invalid/opaque-${channel}`;
    const candidates = [
      route.host ?? url,
      ...(input.relay && route.relay ? [route.relay] : []),
      ...(route.fallback ? [route.fallback] : []),
    ];
    const attempts = Array.from({ length: 5 }, (_, attempt) => {
      const target = candidates[Math.min(Math.floor(attempt / 2), candidates.length - 1)];
      const parsed = new URL(target);
      const relay = parsed.searchParams.get('ts-relay');
      if (relay) parsed.searchParams.delete('ts-relay');
      const headers = { 'X-Venue-Header': 'keep', ...siteHeaders, ...(provider.wsConnection(target)?.headers ?? {}) };
      return { url: relay ? parsed.toString() : target, relay, headers };
    });
    return { channel, original: url, headers: { 'X-Venue-Header': 'keep' }, attempts };
  });
  return { name: input.name, exchange: input.exchange, plan, http, sockets };
};

describe('TS generated native routing golden fixture', () => {
  it('freezes real resolver plans and independent provider destinations for native selectors', async () => {
    mocks.factory.mockReturnValue(mocks.adapter);
    mocks.adapter.mockImplementation(async () => ({ status: 200, headers: {}, data: '{}' }));
    try {
      const matrix = [];
      for (const entry of cases) matrix.push(await buildCase(entry));
      // Opt-in only: changing production routing never rewrites this oracle.
      if (process.env.UPDATE_ENGINE_NATIVE_MATRIX === '1')
        writeFileSync(fixturePath, `${JSON.stringify(matrix, null, 2)}\n`);
      expect(matrix).toEqual(JSON.parse(readFileSync(fixturePath, 'utf8')));
      expect(JSON.stringify(matrix)).not.toMatch(/fixture-proxy-token|fixture-venue-secret/);
    } finally {
      setWsRelayEnabled(false);
    }
  });
});
