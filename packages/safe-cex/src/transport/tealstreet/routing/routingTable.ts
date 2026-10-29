import type { ExchangeName } from '../../../types';
import type { RoutingTable } from './types';

import { isTokyoEgress } from '@tealstreet/http-over-websocket/src/egressRegion';
import { buildRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { exchangeMetadata } from '../../../exchangeMetadata';

// THE declarative routing table. Every value here was previously hardcoded and
// scattered (getPrivateProxy's regex, the HOW TOKYO_EGRESS list, ~18 per-exchange
// ws files). It is transcribed faithfully from that code as of the consolidation;
// transport/tealstreet/routing/__tests__ proves the resolver reproduces the old behavior exactly.
//
// How to read an entry:
//   restPool  - which whitelisted CORS pool REST uses when tealstreetProxy is on.
//   howEgress - HOW (REST-over-WS) tradetunnel egress region (isTokyoEgress -> jp).
//   ws        - per-channel WS host strategy; omitted channels are 'direct'.

// --- WS host literals ---------------------------------------------------------
const TYO = 'wss://fsjnpy143pbz-tyo.tealstreet.io'; // the Vultr tunnel host (AWS_EXIT.md 3e)
// Tokyo passthrough tunnels (Caddy behind Cloudflare) — the DIRECT-path
// fallback hosts for bybit/okx.
const BYBIT_JA = 'wss://bybit-proxy-ja.tealstreet.io';
const OKX_JA = 'wss://okx-proxy-ja.tealstreet.io';
// PRIVATE venue sockets the tradetunnel's rpc.relay may carry (its allowlist is
// host+path-prefix; these must match `RELAY_ALLOWED_UPSTREAMS` on the tunnel).
const RELAY_UPSTREAM = {
  bybitPrivate: 'wss://stream.bybit.com/v5/private',
  okxPrivate: 'wss://ws.okx.com/ws/v5/private',
  okxBusiness: 'wss://ws.okx.com/ws/v5/business',
  okxeuPrivate: 'wss://wseea.okx.com/ws/v5/private',
  okxeuBusiness: 'wss://wseea.okx.com/ws/v5/business',
  bitgetPrivate: 'wss://ws.bitget.com/v2/ws/private',
  bitgetPrivateUta: 'wss://ws.bitget.com/v3/ws/private',
  blofinPrivate: 'wss://openapi.blofin.com/ws/private',
  // The host root: the tunnel matches it since #8265 (an entry ending in `/`).
  coinbasePrivate: 'wss://advanced-trade-ws-user.coinbase.com/',
} as const;
const relay = (upstream: string): string => buildRelayUrl('jp', upstream);

// The browser build's WS hosts. IP-bound private sockets (okx, okxeu, bitget, blofin, coinbase)
// go through the tunnel relay, which dials the venue from the 9-IP fleet; they used
// to be the AWS Python ws-proxy paths (`fsjnpy143pbz.asia…/<venue>/private`), moved
// in AWS_EXIT.md step 3e. bybitbra, grvt and lighter are containers on the tunnel
// hosts (step 5b). The hosted exchange server dials this same map (step 4).
export const BROWSER_WS_HOSTS = {
  grvt: `${TYO}/grvt`,
  okxPrivate: relay(RELAY_UPSTREAM.okxPrivate),
  okxBusiness: relay(RELAY_UPSTREAM.okxBusiness),
  okxeuPrivate: relay(RELAY_UPSTREAM.okxeuPrivate),
  okxeuBusiness: relay(RELAY_UPSTREAM.okxeuBusiness),
  blofin: relay(RELAY_UPSTREAM.blofinPrivate),
  coinbase: relay(RELAY_UPSTREAM.coinbasePrivate),
  bitgetPrivate: relay(RELAY_UPSTREAM.bitgetPrivate),
  bitgetPrivateUta: relay(RELAY_UPSTREAM.bitgetPrivateUta),
  bitgetPublic: 'wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public',
  lighterPublic: `${TYO}/lighter/stream?readonly=true`,
  lighterPrivate: `${TYO}/lighter/stream`,
  lighterRh: `${TYO}/lighter-rh/stream`,
  bybitBraPrivate: `${TYO}/bybitbra/v5/private`,
  bybitBraPublicLinear: `${TYO}/bybitbra/v5/public/linear`,
  bybitBraPublicInverse: `${TYO}/bybitbra/v5/public/inverse`,
  bybitBraPublicSpot: `${TYO}/bybitbra/v5/public/spot`,
  bybitJaPublicLinear: `${BYBIT_JA}/v5/public/linear`,
  bybitJaPublicInverse: `${BYBIT_JA}/v5/public/inverse`,
  bybitJaPublicSpot: `${BYBIT_JA}/v5/public/spot`,
  bybitJaPrivate: `${BYBIT_JA}/v5/private`,
  okxJaPublic: `${OKX_JA}/ws/v5/public`,
  okxJaPrivate: `${OKX_JA}/ws/v5/private`,
  okxJaBusiness: `${OKX_JA}/ws/v5/business`,
  bitunixPublic: 'wss://bitunix-fapi-proxy-ja.tealstreet.io/public/',
  bitunixPrivate: 'wss://bitunix-fapi-proxy-ja.tealstreet.io/private/',
  bitunixFutures: 'wss://bitunix-api-proxy-ja.tealstreet.io/ws-futures/?from=contract&compress=2',
} as const;

// Every WS host the table names. apexomni has none: its private socket connects
// directly (AWS_EXIT.md 5c) — it answers a browser-origin upgrade and binds no IP.
export type WsHosts = Record<keyof typeof BROWSER_WS_HOSTS, string>;

// Exchanges whose REST CORS pool is the Tokyo (jp) box — getPrivateProxy's
// /binance|okx|okex|bitget/ regex. grvt -> 'tiny'; everything else -> 'asia'.
const REST_JP: ExchangeName[] = [
  'binance',
  'binanceinverse',
  'binanceoptions',
  'binancepm',
  'binancespot',
  'okx',
  'okxdemo',
  'okxeu',
  'okxeudemo',
  'mexc',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
];

// Shared WS groups for exchange families whose variants extend the same class and
// therefore reuse the same ws-* classes (and thus the same hosts). Declared once
// and assigned to both parent and variants so they can't drift.
type WsGroup = RoutingTable[ExchangeName]['ws'];

// Exchanges that install a region-pinned HOW adapter from the account's proxyRegion
// (bybit when tunnelling) -> howEgress is 'regionConditional'. Their
// variants that extend the same class inherit it.
// bybit.api.ts's createAPI region-pins the HOW adapter; its consumers are bybit,
// bybitinverse, bybitspot, and bybitdemo (extends BybitExchange {}). HyroTrader
// is Bybit-compatible but direct to a first-party simulator, so it intentionally
// stays out of this list.
const HOW_REGION_CONDITIONAL: ExchangeName[] = ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot'];

// Builds the table over a set of WS hosts: the browser build's, or a test's
// substitute (`setRoutingWsHosts`).
const buildRoutingTable = (WS: WsHosts): RoutingTable => {
  // okx (global entity only — okxeu/okxdemo terminate elsewhere) also carries the
  // athena passthrough as a direct-path fallback on every channel.
  const okxWs: WsGroup = {
    public: { kind: 'direct', fallback: WS.okxJaPublic },
    private: {
      kind: 'proxyConditional',
      host: WS.okxPrivate,
      fallback: WS.okxJaPrivate,
      relayUpstream: RELAY_UPSTREAM.okxPrivate,
    },
    business: {
      kind: 'proxyConditional',
      host: WS.okxBusiness,
      fallback: WS.okxJaBusiness,
      relayUpstream: RELAY_UPSTREAM.okxBusiness,
    },
  };
  // okxeu shares the OKX ws classes but must terminate on the EEA entity's hosts
  // (wseea.okx.com) — EEA credentials do not authenticate on the global ws host.
  const okxeuWs: WsGroup = {
    private: { kind: 'proxyConditional', host: WS.okxeuPrivate },
    business: { kind: 'proxyConditional', host: WS.okxeuBusiness },
  };
  // Bitget checks the key's bound IP on private login (40018), so a proxied
  // account dials its host and then the relay, which egresses from the same fleet
  // as its REST. In the browser build the host IS the relay (AWS_EXIT.md 3e), so
  // the second candidate repeats it.
  const bitgetWs: WsGroup = {
    // bitget.ws-secondary-public reuses the same public fallback host as ws-public.
    public: { kind: 'fallbackOnly', fallback: WS.bitgetPublic, initialSilentConnects: 1 },
    // Master's TS proxy/relay routing. The Rust engine host routes its UTA socket
    // (v3) through privateUta by URL, so no UTA direct bypass remains.
    private: {
      kind: 'proxyConditional',
      host: WS.bitgetPrivate,
      fallback: WS.bitgetPrivate,
      relayUpstream: RELAY_UPSTREAM.bitgetPrivate,
      relayWhenTunnelled: true,
      initialSilentConnects: 1,
      ignoredMessages: ['pong'],
    },
    privateUta: {
      kind: 'proxyConditional',
      host: WS.bitgetPrivateUta,
      fallback: WS.bitgetPrivateUta,
      relayUpstream: RELAY_UPSTREAM.bitgetPrivateUta,
      relayWhenTunnelled: true,
    },
  };
  const coinbaseWs: WsGroup = { private: { kind: 'proxyConditional', host: WS.coinbase } };
  const bybitBrazilPrivateWs = {
    kind: 'bybitBrazilConditional',
    host: WS.bybitBraPrivate,
    fallback: WS.bybitJaPrivate,
    relayUpstream: RELAY_UPSTREAM.bybitPrivate,
  } as const;

  // Per-exchange WS overrides (everything not listed defaults to all-direct).
  const WS_OVERRIDES: Partial<Record<ExchangeName, WsGroup>> = {
    grvt: { private: { kind: 'alwaysProxy', host: WS.grvt } },
    // The bitunix ws classes pick the fallback themselves: `tealstreetWsRoute`, or
    // two connects that delivered nothing. `business` is the frontend futures socket.
    bitunix: {
      public: { kind: 'fallbackOnly', fallback: WS.bitunixPublic },
      private: { kind: 'fallbackOnly', fallback: WS.bitunixPrivate },
      business: { kind: 'fallbackOnly', fallback: WS.bitunixFutures },
    },
    okx: okxWs,
    okxeu: okxeuWs, // OKXEUExchange extends OKXExchange {} but tunnels to wseea.okx.com
    // OKX EU Demo intentionally stays direct: testnet endpoints are wseeapap.okx.com
    // and authenticate with a normal OKX login frame after connect.
    blofin: { private: { kind: 'proxyConditional', host: WS.blofin } },
    coinbase: coinbaseWs,
    coinbasespot: coinbaseWs, // CoinbaseSpotExchange extends CoinbaseExchange
    bitget: bitgetWs,
    bitgetinverse: bitgetWs, // BitgetInverseExchange extends BitgetExchange
    bitgetusdc: bitgetWs, // BitgetUSDCExchange extends BitgetExchange
    lighter: {
      public: { kind: 'wsRouteCandidates', proxy: WS.lighterPublic },
      private: { kind: 'wsRouteCandidates', proxy: WS.lighterPrivate },
    },
    'lighter-robinhood': {
      public: { kind: 'wsRouteCandidates', proxy: WS.lighterRh },
      private: { kind: 'wsRouteCandidates', proxy: WS.lighterRh },
    },
    bybit: {
      public: { kind: 'bybitBrazilConditional', host: WS.bybitBraPublicLinear, fallback: WS.bybitJaPublicLinear },
      private: bybitBrazilPrivateWs,
    },
    bybitinverse: {
      public: { kind: 'bybitBrazilConditional', host: WS.bybitBraPublicInverse, fallback: WS.bybitJaPublicInverse },
      private: bybitBrazilPrivateWs,
    },
    bybitspot: {
      public: { kind: 'bybitBrazilConditional', host: WS.bybitBraPublicSpot, fallback: WS.bybitJaPublicSpot },
      private: bybitBrazilPrivateWs,
    },
  };

  // The set of exchanges is enumerated exactly once, at the type level (ExchangeName)
  // and at runtime by exchangeMetadata's keys — which TypeScript pins to ExchangeName
  // via its `{ [key in ExchangeName]: ... }` mapped type. We derive the table from
  // those keys so a new exchange automatically gets a row (and can't be forgotten):
  // adding it to ExchangeName forces a metadata entry, which forces a routing entry.
  return Object.fromEntries(
    (Object.keys(exchangeMetadata) as ExchangeName[]).map((name) => [
      name,
      {
        restPool: name === 'grvt' ? 'tiny' : REST_JP.includes(name) ? 'jp' : 'asia',
        howEgress: HOW_REGION_CONDITIONAL.includes(name) ? 'regionConditional' : isTokyoEgress(name) ? 'jp' : 'sg',
        ...(WS_OVERRIDES[name] ? { ws: WS_OVERRIDES[name] } : {}),
      },
    ]),
  ) as RoutingTable;
};

let routingTable = buildRoutingTable(BROWSER_WS_HOSTS);

/** Substitutes the WS hosts (tests only; every runtime uses `BROWSER_WS_HOSTS`). */
export const setRoutingWsHosts = (ws: WsHosts): void => {
  routingTable = buildRoutingTable(ws);
};

export const getRoutingTable = (): RoutingTable => routingTable;
