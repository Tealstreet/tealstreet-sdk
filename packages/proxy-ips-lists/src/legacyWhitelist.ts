import {
  privateProxyIpsSingapore,
  privateProxyIpsTokyo,
  privateProxyIpsTokyoSmallPP,
  tokyoK8sNatGatewayIp,
} from './index';

type LegacyWhitelist = { ips: readonly string[]; regions: Record<string, readonly string[]> };

const tokyo: LegacyWhitelist = { ips: privateProxyIpsTokyo, regions: {} };
const bybit: LegacyWhitelist = {
  ips: privateProxyIpsSingapore,
  regions: {
    SG: privateProxyIpsSingapore,
    JA: privateProxyIpsTokyo,
  },
};
const none: LegacyWhitelist = { ips: [], regions: {} };
const tokyoRegion: LegacyWhitelist = {
  ips: privateProxyIpsTokyo,
  regions: {
    JA: privateProxyIpsTokyo,
  },
};
const singapore: LegacyWhitelist = { ips: privateProxyIpsSingapore, regions: {} };
const cryptoCom: LegacyWhitelist = {
  ips: privateProxyIpsTokyo,
  regions: {
    JA: privateProxyIpsTokyo,
    SG: privateProxyIpsSingapore,
  },
};
const grvt: LegacyWhitelist = { ips: [...privateProxyIpsTokyoSmallPP, tokyoK8sNatGatewayIp], regions: {} };

// Display-only projection of legacy-master cb651f89d079914e88a10f24a775f7808241d715
// packages/safe-cex/src/exchangeMetadata.ts; never used for transport routing.
const legacyWhitelist: Record<string, LegacyWhitelist> = {
  asterdex: tokyo,
  backpack: tokyo,
  bybit: bybit,
  bybitdemo: bybit,
  hyrotrader: none,
  bybitspot: bybit,
  bybitinverse: bybit,
  binance: tokyo,
  binanceinverse: tokyoRegion,
  binanceoptions: tokyo,
  binancepm: tokyo,
  binancespot: tokyo,
  bitget: tokyo,
  bitgetinverse: tokyo,
  bitgetusdc: tokyo,
  bitunix: tokyo,
  weex: tokyo,
  mexc: tokyoRegion,
  coinbase: singapore,
  cryptocom: cryptoCom,
  grvt: grvt,
  phemex: singapore,
  bingx: singapore,
  bitrue: tokyo,
  blofin: singapore,
  lighter: singapore,
  'lighter-robinhood': singapore,
  okx: tokyo,
  okxdemo: tokyo,
  okxeu: tokyo,
  okxeudemo: tokyo,
  woo: singapore,
  woofi: singapore,
  coinbasespot: singapore,
  kodiakfi: singapore,
  mode: singapore,
  what: singapore,
  hyperliquid: singapore,
  hyperliquidspot: singapore,
  txflow: singapore,
  apexomni: singapore,
  kucoin: tokyo,
  kucoinspot: tokyo,
  kraken: tokyo,
  krakenspot: tokyo,
  paradex: singapore,
  polymarket: none,
};

export function legacyWhitelistIps(exchange: string, region?: string): readonly string[] {
  const metadata = Object.hasOwn(legacyWhitelist, exchange) ? legacyWhitelist[exchange] : undefined;
  if (!metadata) return [];
  const effectiveRegion = region && Object.hasOwn(metadata.regions, region) ? region : Object.keys(metadata.regions)[0];
  return (effectiveRegion ? metadata.regions[effectiveRegion] : undefined) ?? metadata.ips;
}

export const legacyWhitelistVenues = Object.keys(legacyWhitelist);
export function legacyWhitelistRegions(exchange: string): string[] {
  return Object.hasOwn(legacyWhitelist, exchange) ? Object.keys(legacyWhitelist[exchange].regions) : [];
}

// Same legacy revision, packages/proxy-ips-lists/src/index.ts.
export const legacyExchangesRequireWsIp = ['coinbase', 'blofin', 'okx', 'okxeu', 'bitget'];
