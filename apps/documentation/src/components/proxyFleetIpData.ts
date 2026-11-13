export type ProxyFleetIpGroups = {
  fleet: readonly string[];
  extras: readonly string[];
};

// Docs builds on Vercel run with apps/documentation as the project root, so this
// display-only snapshot must stay self-contained instead of importing workspace packages.
export const proxyFleetWhitelistVenues = [
  'asterdex',
  'backpack',
  'bybit',
  'bybitdemo',
  'hyrotrader',
  'bybitspot',
  'bybitinverse',
  'binance',
  'binanceinverse',
  'binanceoptions',
  'binancepm',
  'binancespot',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
  'bitunix',
  'weex',
  'mexc',
  'coinbase',
  'cryptocom',
  'grvt',
  'phemex',
  'bingx',
  'bitrue',
  'blofin',
  'lighter',
  'lighter-robinhood',
  'okx',
  'okxdemo',
  'okxeu',
  'okxeudemo',
  'woo',
  'woofi',
  'coinbasespot',
  'kodiakfi',
  'mode',
  'what',
  'hyperliquid',
  'hyperliquidspot',
  'txflow',
  'apexomni',
  'kucoin',
  'kucoinspot',
  'kraken',
  'krakenspot',
  'paradex',
  'polymarket',
] as const;

// Snapshot from packages/proxy-ips-lists/src/index.ts and src/whitelist.ts.
const proxyFleetIpsVultrTokyo = [
  '167.179.112.217',
  '45.63.121.235',
  '45.77.135.138',
  '108.61.162.122',
  '149.28.30.214',
  '108.160.129.108',
  '45.77.27.69',
  '45.76.52.241',
  '108.61.247.61',
] as const;

const bitunixPassthroughIps = ['72.46.86.143', '202.182.126.32'] as const;

export const docsWhitelistIpGroups = (exchange = ''): ProxyFleetIpGroups => ({
  fleet: proxyFleetIpsVultrTokyo,
  extras: exchange === 'bitunix' ? bitunixPassthroughIps : [],
});
