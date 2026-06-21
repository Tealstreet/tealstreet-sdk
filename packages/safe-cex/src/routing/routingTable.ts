import type { ExchangeName } from '../types';
import type { RoutingTable, WsStrategy } from './types';

import { isTokyoEgress } from '@tealstreet/http-over-websocket/src/egressRegion';

// THE declarative routing table. Every value here was previously hardcoded and
// scattered (getPrivateProxy's regex, the HOW TOKYO_EGRESS list, ~18 per-exchange
// ws files). It is transcribed faithfully from that code as of the consolidation;
// routing/__tests__ proves the resolver reproduces the old behavior exactly.
//
// How to read an entry:
//   restPool  - which whitelisted CORS pool REST uses when tealstreetProxy is on.
//   howEgress - HOW (REST-over-WS) tradetunnel egress region (TOKYO_EGRESS list -> jp).
//   ws        - per-channel WS host strategy; omitted channels are 'direct'.

// --- WS host literals (exact, matching the current per-exchange ws classes) ---
const T = 'wss://fsjnpy143pbz'; // tunnel host prefix
const JP = `${T}.jp.tealstreet.io`;
const ASIA = `${T}.asia.tealstreet.io`;

const WS = {
  arkham: `${JP}/arkm`,
  grvt: `${JP}/grvt`,
  okxPrivate: `${ASIA}/okx/private`,
  okxBusiness: `${ASIA}/okx/business`,
  blofin: `${ASIA}/blofin/private`,
  coinbase: `${ASIA}/coinbase/private`,
  apex: `${ASIA}/apex/private`,
  bitmexJa: `${JP}/bitmex/private`,
  bitmexOther: `${ASIA}/bitmex/private`,
  bitgetPrivate: `${ASIA}/bitget/v2/private`,
  bitgetPublic: 'wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public',
  lighterPublic: `${JP}/lighter/stream?readonly=true`,
  lighterPrivate: `${JP}/lighter/stream`,
} as const;

const direct: WsStrategy = { kind: 'direct' };

// Exchanges whose REST CORS pool is the Tokyo (jp) box — getPrivateProxy's
// /binance|okx|okex|bitget/ regex. grvt -> 'tiny'; everything else -> 'asia'.
const REST_JP: ExchangeName[] = [
  'binance',
  'binancepm',
  'binancespot',
  'okx',
  'okxeu',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
];

// Shared WS groups for exchange families whose variants extend the same class and
// therefore reuse the same ws-* classes (and thus the same hosts). Declared once
// and assigned to both parent and variants so they can't drift.
type WsGroup = RoutingTable[ExchangeName]['ws'];
const okxWs: WsGroup = {
  private: { kind: 'proxyConditional', host: WS.okxPrivate },
  business: { kind: 'proxyConditional', host: WS.okxBusiness },
};
const bitgetWs: WsGroup = {
  // bitget.ws-secondary-public reuses the same public fallback host as ws-public.
  public: { kind: 'fallbackOnly', fallback: WS.bitgetPublic },
  private: { kind: 'fallbackOnly', fallback: WS.bitgetPrivate },
};
const coinbaseWs: WsGroup = { private: { kind: 'proxyConditional', host: WS.coinbase } };

// Per-exchange WS overrides (everything not listed defaults to all-direct).
const WS_OVERRIDES: Partial<Record<ExchangeName, WsGroup>> = {
  arkham: { public: { kind: 'alwaysProxy', host: WS.arkham }, private: { kind: 'alwaysProxy', host: WS.arkham } },
  grvt: { private: { kind: 'alwaysProxy', host: WS.grvt } },
  okx: okxWs,
  okxeu: okxWs, // OKXEUExchange extends OKXExchange {}
  blofin: { private: { kind: 'proxyConditional', host: WS.blofin } },
  coinbase: coinbaseWs,
  coinbasespot: coinbaseWs, // CoinbaseSpotExchange extends CoinbaseExchange
  apexomni: { private: { kind: 'proxyConditional', host: WS.apex } },
  bitmex: { private: { kind: 'regionConditional', ja: WS.bitmexJa, other: WS.bitmexOther } },
  bitget: bitgetWs,
  bitgetinverse: bitgetWs, // BitgetInverseExchange extends BitgetExchange
  bitgetusdc: bitgetWs, // BitgetUSDCExchange extends BitgetExchange
  lighter: {
    public: { kind: 'wsRouteCandidates', proxy: WS.lighterPublic },
    private: { kind: 'wsRouteCandidates', proxy: WS.lighterPrivate },
  },
};

// Exchanges that install a region-pinned HOW adapter from the account's proxyRegion
// (bitmex always; bybit when tunnelling) -> howEgress is 'regionConditional'. Their
// variants that extend the same class inherit it.
// bybit.api.ts's createAPI region-pins the HOW adapter; its consumers are bybit,
// bybitinverse, bybitspot, and bybitdemo (extends BybitExchange {}). bitmex.api.ts
// likewise -> bitmex + bitmextestnet.
const HOW_REGION_CONDITIONAL: ExchangeName[] = [
  'bitmex',
  'bitmextestnet',
  'bybit',
  'bybitdemo',
  'bybitinverse',
  'bybitspot',
];

const ALL_EXCHANGES = [
  'apexomni',
  'arkham',
  'asterdex',
  'backpack',
  'binance',
  'binancespot',
  'binancepm',
  'bingx',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
  'bitmex',
  'bitmextestnet',
  'bitrue',
  'bitunix',
  'blofin',
  'breakoutprop',
  'btcc',
  'bybit',
  'bybitdemo',
  'bybitinverse',
  'bybitspot',
  'coinbase',
  'coinbasespot',
  'cryptocom',
  'dydx',
  'grvt',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  'okx',
  'okxeu',
  'paradex',
  'phemex',
  'polymarket',
  'kodiakfi',
  'mode',
  'salsadex',
  'tealstreet-dex',
  'what',
  'weex',
  'woo',
  'woofi',
  'kucoin',
  'kucoinspot',
  'kraken',
  'krakenspot',
] as const satisfies readonly ExchangeName[];

export const routingTable: RoutingTable = Object.fromEntries(
  ALL_EXCHANGES.map((name) => [
    name,
    {
      restPool: name === 'grvt' ? 'tiny' : REST_JP.includes(name) ? 'jp' : 'asia',
      howEgress: HOW_REGION_CONDITIONAL.includes(name) ? 'regionConditional' : isTokyoEgress(name) ? 'jp' : 'sg',
      ...(WS_OVERRIDES[name] ? { ws: WS_OVERRIDES[name] } : {}),
    },
  ]),
) as RoutingTable;

// bitmextestnet mirrors bitmex (testnet is out of scope but the key must exist).
routingTable.bitmextestnet.ws = routingTable.bitmex.ws;

// Compile-time exhaustiveness: a new ExchangeName cannot be added without listing
// it in ALL_EXCHANGES above — this line errors and names the missing exchange.
type _MissingExchange = Exclude<ExchangeName, (typeof ALL_EXCHANGES)[number]>;
const _assertAllExchangesCovered: _MissingExchange extends never ? true : _MissingExchange = true;
void _assertAllExchangesCovered;

export { direct };
