import type { ExchangeAccountConfig, ExchangeName, ExchangeOptions } from '../types';

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import { TOKYO_EGRESS_EXCHANGES } from '@tealstreet/http-over-websocket/src/egressRegion';
import { RELAY_QUERY_PARAM, tunnelEgressUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { accountToExchangeOptions } from '../createExchangeFromAccount';
import { normalizeExchangeAccountConfig } from '../accounts/normalizeExchangeAccountConfig';
import { exchangeMetadata, getExchangeMetadata } from '../exchangeMetadata';
import { directTransport } from './direct';
import { resolveEngineRoutingPlan } from './engine';
import { getRoutingTable } from './tealstreet/routing/routingTable';
import { TEALSTREET_REST_HEADERS, TEALSTREET_WS_HEADERS, tealstreetTransport } from './tealstreet/tealstreetTransport';
import { getPrivateProxyHost, getPublicCorsProxy } from './tealstreet/transportResolution';

// The Rust resolver (sdk-rs/hosts/native/src/routing/resolve.rs) reads the table
// and is held to these plans, which use the hosted relay gate (off).
// Regenerate both with UPDATE_NATIVE_ROUTING_PORT=1.
const mocks = vi.hoisted(() => ({ adapter: vi.fn() }));
vi.mock('../accessToken', () => ({
  getProxyToken: async () => '',
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => async () => '',
}));
vi.mock('../httpOverWebsocket/axiosHttpOverWebsocketAdapter', () => ({
  axiosHttpOverWebsocketAdapter: mocks.adapter,
  bigIntAxiosHttpOverWebsocketAdapter: mocks.adapter,
  createAxiosHttpOverWebsocketAdapter: () => mocks.adapter,
}));

const root = resolve(__dirname, '../../../../sdk-rs/hosts/native');
const tablePath = resolve(root, 'src/routing/table.json');
const plansPath = resolve(root, 'tests/fixtures/routing-port-plans.json');

const BASE_URLS: Partial<Record<ExchangeName, string>> = {
  binance: 'https://fapi.binance.com',
  binancepm: 'https://papi.binance.com',
  binancespot: 'https://api.binance.com',
  binanceinverse: 'https://dapi.binance.com',
  bybit: 'https://api.bybit.com',
  bybitinverse: 'https://api.bybit.com',
  bybitspot: 'https://api.bybit.com',
  bybitdemo: 'https://api-demo.bybit.com',
  hyrotrader: 'https://api.hyrotrader.com',
  okx: 'https://www.okx.com',
  okxeu: 'https://eea.okx.com',
  okxdemo: 'https://www.okx.com',
  okxeudemo: 'https://eea.okx.com',
  bitget: 'https://api.bitget.com',
  bitgetusdc: 'https://api.bitget.com',
  bitgetinverse: 'https://api.bitget.com',
  hyperliquid: 'https://api.hyperliquid.xyz',
  hyperliquidspot: 'https://api.hyperliquid.xyz',
  blofin: 'https://openapi.blofin.com',
  bingx: 'https://open-api.bingx.com',
  lighter: 'https://mainnet.zklighter.elliot.ai',
  'lighter-robinhood': 'https://mainnet.zklighter.elliot.ai',
  paradex: 'https://api.prod.paradex.trade',
  phemex: 'https://api.phemex.com',
  bitunix: 'https://fapi.bitunix.com',
  asterdex: 'https://fapi.asterdex.com',
  grvt: 'https://trades.grvt.io',
};

/** Saved-account transport preferences, as the hosted init payload carries them. */
const ACCOUNTS: Array<Partial<ExchangeAccountConfig>> = [
  {},
  { tealstreetProxy: true },
  { tealstreetProxy: true, proxyRegion: 'JA' },
  { tealstreetProxy: true, proxyRegion: 'SG' },
  { tealstreetTunnel: true },
  { tealstreetTunnel: false },
  { tealstreetProxy: true, tealstreetTunnel: true, proxyRegion: 'JA' },
  { tealstreetWsRoute: true },
  { testnet: true, tealstreetProxy: true },
];

/** Direct option sets that the account path never produces but the resolver accepts. */
const OPTIONS: Array<Partial<ExchangeOptions>> = [
  { publicDataOnly: true },
  { publicDataOnly: true, tealstreetProxy: true, tealstreetTunnel: true },
  { tealstreetProxy: true, overrideProxyUrl: 'https://override.invalid/proxy' },
  { bybitSiteId: 'BRA_BTL' },
  { tealstreetProxy: true, tealstreetWsRoute: true, proxyRegion: 'JA' },
];

const transportOptions = (options: Partial<ExchangeOptions>) => ({
  tealstreetProxy: options.tealstreetProxy,
  tealstreetTunnel: options.tealstreetTunnel,
  tealstreetWsRoute: options.tealstreetWsRoute,
  proxyRegion: options.proxyRegion,
  bybitSiteId: options.bybitSiteId,
  publicDataOnly: options.publicDataOnly,
  hasKey: Boolean(options.key),
  payloadSigner: Boolean(options.payloadSigner),
  overrideProxyUrl: options.overrideProxyUrl,
});

const plan = (exchange: ExchangeName, options: ExchangeOptions, baseURL: string) => {
  try {
    return { plan: resolveEngineRoutingPlan(exchange, options, baseURL) };
  } catch (error) {
    return { error: (error as Error).message };
  }
};

const buildTable = () => ({
  hosts: {
    privateProxy: { jp: getPrivateProxyHost('jp'), asia: getPrivateProxyHost('asia'), tiny: getPrivateProxyHost('tiny') },
    publicCorsProxy: getPublicCorsProxy(),
    tunnel: { jp: tunnelEgressUrl('jp'), sg: tunnelEgressUrl('sg') },
    relayQueryParam: RELAY_QUERY_PARAM,
  },
  restHeaders: TEALSTREET_REST_HEADERS,
  wsHeaders: TEALSTREET_WS_HEADERS,
  tokyoEgress: [...TOKYO_EGRESS_EXCHANGES],
  exchanges: Object.fromEntries(
    (Object.keys(exchangeMetadata) as ExchangeName[]).sort().map((exchange) => {
      const metadata = getExchangeMetadata(exchange);
      return [
        exchange,
        {
          routing: getRoutingTable()[exchange],
          requiresCors: Boolean(metadata.requiresCors),
          enforceWsTunnel: Boolean(metadata.enforceWsTunnel),
          supportsTunnel: metadata.supportsTunnel ?? null,
          privateProxyRegions: Object.keys(metadata.privateProxyRegions || {}),
        },
      ];
    }),
  ),
});

const buildPlans = () => {
  const cases: unknown[] = [];
  for (const [exchange, baseURL] of Object.entries(BASE_URLS) as Array<[ExchangeName, string]>) {
    for (const preferences of ACCOUNTS) {
      const account = normalizeExchangeAccountConfig({
        exchange,
        name: 'fixture',
        apiKey: 'fixture-key',
        apiSecret: '',
        ...preferences,
      } as ExchangeAccountConfig);
      const options = { ...accountToExchangeOptions(account), transport: tealstreetTransport };
      cases.push({
        exchange,
        baseURL,
        account: preferences,
        provider: 'tealstreet',
        options: transportOptions(options),
        ...plan(exchange, options, baseURL),
      });
    }
    for (const extra of OPTIONS) {
      for (const [provider, transport] of [
        ['tealstreet', tealstreetTransport],
        ['direct', directTransport],
      ] as const) {
        const options = { key: 'fixture-key', ...extra, transport } as ExchangeOptions;
        cases.push({ exchange, baseURL, provider, options: transportOptions(options), ...plan(exchange, options, baseURL) });
      }
    }
  }
  return cases;
};

describe('native routing port fixtures', () => {
  it('freezes the routing table and TS plans the Rust resolver must reproduce', () => {
    const table = buildTable();
    const plans = buildPlans();
    if (process.env.UPDATE_NATIVE_ROUTING_PORT === '1') {
      writeFileSync(tablePath, `${JSON.stringify(table, null, 2)}\n`);
      writeFileSync(plansPath, `${JSON.stringify(plans, null, 2)}\n`);
    }
    expect(table).toEqual(JSON.parse(readFileSync(tablePath, 'utf8')));
    expect(plans).toEqual(JSON.parse(readFileSync(plansPath, 'utf8')));
  });
});
