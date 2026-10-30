import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';
import type { ResolvedWs } from './types';

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import { BASE_WS_PRIVATE_URL, BASE_WS_PRIVATE_UTA_URL, BASE_WS_URL } from '../exchanges/bitget/bitget.types';
import { directTransport } from './direct';
import { createEngineTransport, resolveEngineRoutingPlan } from './engine';
import { resolveExchangeRouting } from './tealstreet/routing/resolveExchangeRouting';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { setWsRelayEnabled } from './tealstreet/wsRelay';

const mocks = vi.hoisted(() => ({ adapter: vi.fn(), factory: vi.fn() }));
vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: async () => 'fixture-proxy-token',
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  // The real binder looks the token up through getProxyToken.
  bindProxyTokenLookup: () => async () => 'fixture-proxy-token',
}));
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
  bitget?: 'classic' | 'uta' | 'pong' | 'ack' | 'recover' | 'direct';
  bybitProducts?: boolean;
}

const cases: Case[] = [
  {
    name: 'bybit-brazil-product-public-sockets', exchange: 'bybit', baseURL: 'https://api.bybit.com',
    options: { bybitSiteId: 'BRA_BTL', tealstreetTunnel: false }, bybitProducts: true,
  },
  { name: 'kucoin-private-default-how', exchange: 'kucoin', baseURL: 'https://api-futures.kucoin.com', options: {}, how: true },
  { name: 'kucoin-private-explicit-false-how', exchange: 'kucoin', baseURL: 'https://api-futures.kucoin.com', options: { tealstreetTunnel: false }, how: true },
  { name: 'kucoin-public-direct', exchange: 'kucoin', baseURL: 'https://api-futures.kucoin.com', options: { publicDataOnly: true, tealstreetTunnel: true } },
  { name: 'kucoinspot-private-default-how', exchange: 'kucoinspot', baseURL: 'https://api.kucoin.com', options: {}, how: true },
  { name: 'kucoinspot-private-explicit-false-how', exchange: 'kucoinspot', baseURL: 'https://api.kucoin.com', options: { tealstreetTunnel: false }, how: true },
  { name: 'kucoinspot-public-direct', exchange: 'kucoinspot', baseURL: 'https://api.kucoin.com', options: { publicDataOnly: true, tealstreetTunnel: true } },
  {
    name: 'kucoinspot-secondary-private-how', exchange: 'kucoinspot', baseURL: 'https://api.kucoin.com', how: true,
    options: { tealstreetTunnel: false, passphrase: 'fixture-primary-pass', secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret', secondary_passphrase: 'fixture-read-pass', secondaryProxyUrl: 'https://secondary.invalid/spot' },
  },
  {
    name: 'kucoinspot-secondary-public-cors', exchange: 'kucoinspot', baseURL: 'https://api.kucoin.com',
    options: { publicDataOnly: true, tealstreetProxy: true, passphrase: 'fixture-primary-pass', secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret', secondary_passphrase: 'fixture-read-pass', secondaryProxyUrl: 'https://secondary.invalid/spot' },
  },
  {
    name: 'kucoinspot-secondary-direct-provider', exchange: 'kucoinspot', baseURL: 'https://api.kucoin.com', direct: true,
    options: { passphrase: 'fixture-primary-pass', secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret', secondary_passphrase: 'fixture-read-pass' },
  },
  {
    name: 'bitunix-secondary-private-cors', exchange: 'bitunix', baseURL: 'https://fapi.bitunix.com',
    options: { tealstreetProxy: true, tealstreetTunnel: false, secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret', secondaryProxyUrl: 'https://secondary.invalid/bitunix' },
  },
  {
    name: 'bitunix-secondary-private-how', exchange: 'bitunix', baseURL: 'https://fapi.bitunix.com', how: true,
    options: { tealstreetTunnel: true, secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret', secondaryProxyUrl: 'https://secondary.invalid/bitunix' },
  },
  {
    name: 'bitunix-secondary-public-cors', exchange: 'bitunix', baseURL: 'https://fapi.bitunix.com',
    options: { publicDataOnly: true, tealstreetProxy: true, secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret', secondaryProxyUrl: 'https://secondary.invalid/bitunix' },
  },
  {
    name: 'bitunix-secondary-direct-provider', exchange: 'bitunix', baseURL: 'https://fapi.bitunix.com', direct: true,
    options: { secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret' },
  },
  { name: 'breakout-direct-provider', exchange: 'breakoutprop', baseURL: 'https://breakout.prosp.devexperts.com', options: {}, direct: true },
  { name: 'breakout-tealstreet-direct', exchange: 'breakoutprop', baseURL: 'https://breakout.prosp.devexperts.com', options: {} },
  { name: 'breakout-proxy', exchange: 'breakoutprop', baseURL: 'https://breakout.prosp.devexperts.com', options: { tealstreetProxy: true, tealstreetTunnel: false } },
  { name: 'breakout-how', exchange: 'breakoutprop', baseURL: 'https://breakout.prosp.devexperts.com', options: { tealstreetTunnel: true }, how: true },
  { name: 'coinbase-direct-jwt', exchange: 'coinbase', baseURL: 'https://api.coinbase.com', options: {} },
  { name: 'coinbase-proxy-jwt', exchange: 'coinbase', baseURL: 'https://api.coinbase.com', options: { tealstreetProxy: true } },
  { name: 'coinbasespot-proxy-jwt', exchange: 'coinbasespot', baseURL: 'https://api.coinbase.com', options: { tealstreetProxy: true } },
  { name: 'coinbase-how-jwt', exchange: 'coinbase', baseURL: 'https://api.coinbase.com', options: { tealstreetProxy: true, tealstreetTunnel: true }, how: true },
  { name: 'bitunix-force-ws', exchange: 'bitunix', baseURL: 'https://fapi.bitunix.com', options: { tealstreetWsRoute: true } },
  { name: 'bitunix-direct-first-ws', exchange: 'bitunix', baseURL: 'https://fapi.bitunix.com', options: { tealstreetWsRoute: false } },
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
  { name: 'binancespot-public-private', exchange: 'binancespot', baseURL: 'https://api.binance.com', options: { tealstreetProxy: true, tealstreetTunnel: false } },
  { name: 'binanceinverse-public-private', exchange: 'binanceinverse', baseURL: 'https://dapi.binance.com', options: { tealstreetProxy: true, tealstreetTunnel: false } },
  { name: 'binancespot-secondary-public-private', exchange: 'binancespot', baseURL: 'https://api.binance.com', options: { tealstreetProxy: true, tealstreetTunnel: false, secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret' } },
  { name: 'binanceinverse-secondary-public-private', exchange: 'binanceinverse', baseURL: 'https://dapi.binance.com', options: { tealstreetProxy: true, tealstreetTunnel: false, secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret' } },
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
  {
    name: 'bitget-classic-feeds',
    exchange: 'bitget',
    baseURL: 'https://api.bitget.com',
    options: {},
    bitget: 'classic',
  },
  { name: 'bitget-uta-direct', exchange: 'bitget', baseURL: 'https://api.bitget.com', options: {}, bitget: 'uta' },
  { name: 'bitget-private-pong', exchange: 'bitget', baseURL: 'https://api.bitget.com', options: {}, bitget: 'pong' },
  { name: 'bitget-private-login', exchange: 'bitget', baseURL: 'https://api.bitget.com', options: {}, bitget: 'ack' },
  {
    name: 'bitget-write-recovery',
    exchange: 'bitget',
    baseURL: 'https://api.bitget.com',
    options: {},
    bitget: 'recover',
  },
  {
    name: 'bitget-direct-recovery',
    exchange: 'bitget',
    baseURL: 'https://api.bitget.com',
    options: {},
    bitget: 'direct',
    direct: true,
  },
  ...([undefined, false, true] as const).flatMap((publicDataOnly) =>
    ([undefined, false, true] as const).map((tealstreetTunnel) => ({
      name: `hyperliquid-public-${publicDataOnly}-tunnel-${tealstreetTunnel}`,
      exchange: 'hyperliquid' as const,
      baseURL: 'https://api.hyperliquid.xyz',
      options: { publicDataOnly, tealstreetTunnel },
      how: !publicDataOnly,
    })),
  ),
  {
    name: 'hyperliquid-vault-only-raw-undefined',
    exchange: 'hyperliquid',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { key: '', secret: '', vault: '0x1111111111111111111111111111111111111111', tealstreetTunnel: false },
    how: true,
  },
  {
    name: 'hyperliquid-address-only-raw-undefined',
    exchange: 'hyperliquid',
    baseURL: 'https://api.hyperliquid.xyz',
    options: {
      key: '',
      secret: '',
      applicationId: '0x2222222222222222222222222222222222222222',
      tealstreetTunnel: false,
    },
    how: true,
  },
  {
    name: 'hyperliquid-keyless-raw-undefined',
    exchange: 'hyperliquid',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { key: '', secret: '', tealstreetTunnel: false },
    how: true,
  },
  {
    name: 'hyperliquidspot-private-false-tunnel',
    exchange: 'hyperliquidspot',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { tealstreetTunnel: false },
    how: true,
  },
  {
    name: 'hyperliquidspot-public-true-tunnel',
    exchange: 'hyperliquidspot',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { publicDataOnly: true, tealstreetTunnel: true },
  },
  {
    name: 'hyperliquid-how-wraps-cors-proxy',
    exchange: 'hyperliquid',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { tealstreetProxy: true, tealstreetTunnel: false },
    how: true,
  },
  {
    name: 'hyperliquid-public-cors-proxy',
    exchange: 'hyperliquid',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { publicDataOnly: true, tealstreetProxy: true, tealstreetTunnel: false },
  },
  {
    name: 'hyperliquid-how-override',
    exchange: 'hyperliquid',
    baseURL: 'https://api.hyperliquid.xyz',
    options: { tealstreetTunnel: false, overrideProxyUrl: 'https://override.invalid/route' },
    how: true,
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
  const keyModes = input.exchange.startsWith('binance') || input.exchange === 'bitunix' ? [false, true] : [false];
  for (const keyed of keyModes) {
    const url = `${input.baseURL}/fixture?signature=a%2Fb`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...(keyed ? { [input.exchange === 'bitunix' ? 'api-key' : 'x-mbx-apikey']: options.secondary_key ?? 'fixture-venue-key' } : {}),
      ...(input.exchange === 'breakoutprop' ? { Authorization: 'DXAPI documented-session' } : {}),
      ...(['coinbase', 'coinbasespot'].includes(input.exchange) ? { Authorization: 'Bearer fixture-venue-jwt' } : {}),
    };
    const publicBinance = ['binance', 'binancepm', 'binanceinverse'].includes(input.exchange) && !keyed;
    const publicRequest = input.exchange.startsWith('binance') && !keyed;
    const read = Boolean(options.secondary_key) && !publicBinance;
    // The dynamic TS factory selects read credentials/options before its Spot
    // public interceptor strips the key. Ask the real provider independently.
    const selectedOptions = read ? {
      ...options,
      key: options.secondary_key!,
      secret: options.secondary_secret!,
      passphrase: options.secondary_passphrase ?? options.passphrase,
      secondary_key: undefined,
      secondary_secret: undefined,
      secondary_passphrase: undefined,
      overrideProxyUrl: options.secondaryProxyUrl || provider.publicCorsProxy(),
    } : options;
    // Reproduce the TS public-client bypass; other requests go through the real
    // provider independently, without passing the engine's pre-resolved plan.
    const route = publicBinance
      ? { baseURL: url, proxied: false }
      : provider.rest({ exchange: input.exchange, options: selectedOptions, config: { baseURL: url, headers } });
    const target = route.baseURL ?? url;
    const prefix = target === url ? '' : target.slice(0, -url.length - 1);
    const selected = read
      ? publicRequest ? plan.readPublicHttp ?? plan.readHttp! : plan.readHttp!
      : publicRequest ? plan.publicHttp! : plan.http;
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
    const expectedAliases: Record<string, string> = ['coinbase', 'coinbasespot'].includes(input.exchange) && route.proxied && !input.how
      ? { Authorization: 'Coinbase-Authorization' } : {};
    expect(selected.headerAliases ?? {}, input.name).toEqual(expectedAliases);
    for (const [source, target] of Object.entries(expectedAliases)) {
      expectedHeaders[target] = expectedHeaders[source];
      delete expectedHeaders[source];
    }
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
        ...(Object.keys(expectedAliases).length ? { headerAliases: expectedAliases } : {}),
        tunnelUrl: selected.tunnelUrl ?? null,
      },
    });
  }
  const socketInputs: Array<{
    channel: 'public' | 'private' | 'business';
    url: string;
    context?: { feed: string; recover: boolean };
  }> = input.bitget
    ? [
        ...(input.bitget === 'classic'
          ? ['primary', 'secondary-1', 'secondary-2'].map((feed) => ({
              channel: 'public' as const,
              url: BASE_WS_URL,
              context: { feed, recover: false },
            }))
          : []),
        {
          channel: 'private',
          url: input.bitget === 'uta' ? BASE_WS_PRIVATE_UTA_URL : BASE_WS_PRIVATE_URL,
          context: { feed: 'private', recover: false },
        },
      ]
    : input.bybitProducts
      ? ['linear', 'inverse', 'spot', 'option'].map((product) => ({
          channel: 'public' as const,
          url: `wss://stream.bybit.com/v5/public/${product}`,
        }))
      : (['public', 'private', 'business'] as const).map((channel) => ({
          channel,
          url: `wss://venue.invalid/opaque-${channel}`,
        }));
  // Independent oracle: TS's inverse/spot public classes route by their own names; no option proxy exists.
  const productRouting = (url: string): Partial<ResolvedWs> | undefined =>
    !input.bybitProducts
      ? undefined
      : url.endsWith('/inverse')
        ? resolveExchangeRouting('bybitinverse', options).wsPublic
        : url.endsWith('/spot')
          ? resolveExchangeRouting('bybitspot', options).wsPublic
          : url.endsWith('/option')
            ? {}
            : undefined;
  const sockets = socketInputs.map(({ channel, url, context }) => {
    // TS's Bitget private class routes its UTA (v3) socket through privateUta.
    const utaRouting =
      channel === 'private' && url.includes('/v3/ws/private') && routing.wsPrivateUta
        ? { ...routing.wsPrivateUta, ignoredMessages: routing.wsPrivate.ignoredMessages }
        : undefined;
    const declared =
      productRouting(url) ??
      utaRouting ??
      routing[channel === 'public' ? 'wsPublic' : channel === 'private' ? 'wsPrivate' : 'wsBusiness'];
    // TS Bitunix socket classes select fallback immediately on this explicit flag.
    const forced = input.exchange === 'bitunix' && options.tealstreetWsRoute && declared.fallback;
    const route: Partial<ResolvedWs> = declared.directUrls?.includes(url) ? {} : forced ? { host: declared.fallback } : declared;
    const candidates = [
      route.host ?? url,
      ...(input.relay && route.relay ? [route.relay] : []),
      ...(route.fallback ? [route.fallback] : []),
    ];
    const attempts = Array.from({ length: 5 }, (_, attempt) => {
      const recover = input.bitget === 'recover' || input.bitget === 'direct' || input.bitget === 'uta';
      const message =
        input.bitget === 'pong' ? 'pong' : input.bitget === 'ack' ? '{"event":"login","code":"0"}' : undefined;
      const proved = message !== undefined && !route.ignoredMessages?.includes(message);
      const index = recover
        ? candidates.length - 1
        : proved
          ? 0
          : Math.floor((attempt + (route.initialSilentConnects ?? 0)) / 2);
      const target = candidates[Math.min(index, candidates.length - 1)];
      const parsed = new URL(target);
      const relay = parsed.searchParams.get('ts-relay');
      if (relay) parsed.searchParams.delete('ts-relay');
      const headers = { 'X-Venue-Header': 'keep', ...siteHeaders, ...(provider.wsConnection(target)?.headers ?? {}) };
      return {
        url: relay ? parsed.toString() : target,
        relay,
        headers,
        ...(recover && attempt === 0 && context ? { context: { ...context, recover: true } } : {}),
        ...(message !== undefined ? { message } : {}),
      };
    });
    return { channel, original: url, headers: { 'X-Venue-Header': 'keep' }, ...(context ? { context } : {}), attempts };
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
      expect(JSON.stringify(matrix)).not.toMatch(/fixture-proxy-token|fixture-venue-secret|fixture-read-secret/);
    } finally {
      setWsRelayEnabled(false);
    }
  });
});
