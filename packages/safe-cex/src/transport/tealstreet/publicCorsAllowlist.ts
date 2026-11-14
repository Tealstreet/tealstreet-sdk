import type { ExchangeName } from '../../types';

// The hosts /api/public_proxy/ forwards: the edge Worker's PUBLIC_HOSTS, which
// infra/tofu/proxy-fleet/public-cors.tf derives from terraform/exchange-proxy-haproxy-tokyo.cfg.
// Any other target is refused there; publicCorsAllowlist.test.ts keeps this equal.
export const PUBLIC_CORS_HOSTS: readonly string[] = [
  'advanced-trade-ws.coinbase.com',
  'api-demo.bybit.com',
  'api-evm.orderly.org',
  'api-futures.kucoin.com',
  'api-pub.woo.network',
  'api-pub.woo.org',
  'api-pub.woox.io',
  'api-swap-rest.bingx.com',
  'api-testnet.bybit.com',
  'api.aster.fi',
  'api.backpack.exchange',
  'api.binance.com',
  'api.bitget.com',
  'api.bybit.com',
  'api.coinbase.com',
  'api.coincatch.com',
  'api.hyperliquid-testnet.xyz',
  'api.hyperliquid.xyz',
  'api.international.coinbase.com',
  'api.kucoin.com',
  'api.mexc.com',
  'api.phemex.com',
  'api.prod.paradex.trade',
  'api.rh.lighter.xyz',
  'api.testnet.paradex.trade',
  'api.woo.network',
  'api.woo.org',
  'api.woox.io',
  'arkm.com',
  'aws.okx.com',
  'contract.mexc.com',
  'dapi.binance.com',
  'demo-futures.kraken.com',
  'dstream.binance.com',
  'edge.grvt.io',
  'edge.testnet.grvt.io',
  'eea.okx.com',
  'fapi.asterdex.com',
  'fapi.binance.com',
  'fapi.bitrue.com',
  'fapi.bitunix.com',
  'fapiws-auth.bitrue.com',
  'fstream.asterdex.com',
  'fstream.binance.com',
  'futures.kraken.com',
  'icanhazip.com',
  'mainnet.zklighter.elliot.ai',
  'market-data.grvt.io',
  'market-data.testnet.grvt.io',
  'omni.apex.exchange',
  'open-api-swap.bingx.com',
  'open-api.bingx.com',
  'openapi.blofin.com',
  'papi.binance.com',
  'pro.apex.exchange',
  'stream-demo.bybit.com',
  'stream.aster.fi',
  'stream.binance.com',
  'stream.bybit.com',
  'testnet-api.aster.fi',
  'testnet-api.phemex.com',
  'testnet-stream.aster.fi',
  'testnet.omni.apex.exchange',
  'testnet.pro.apex.exchange',
  'trades.grvt.io',
  'trades.testnet.grvt.io',
  'vapi.phemex.com',
  'ws-evm.orderly.org',
  'ws-market-swap.qq-os.com',
  'ws-private-evm.orderly.org',
  'ws.backpack.exchange',
  'ws.bitget.com',
  'ws.okx.com',
  'ws.okx.com:8443',
  'ws.phemex.com',
  'ws.prod.paradex.trade',
  'ws.testnet.paradex.trade',
  'wsaws.okx.com',
  'wseea.okx.com',
  'wseea.okx.com:8443',
  'wss.woo.network',
  'wss.woo.org',
  'wss.woox.io',
  'www.okx.com',
];

/**
 * Venues whose keyless chart source reads through the public CORS proxy, each with the public
 * REST hosts it may send there. Opt-in: a host must be on PUBLIC_CORS_HOSTS, and a request to any
 * other host (BloFin demo, a testnet the list lacks) keeps the exchange proxy.
 */
export const PUBLIC_PROXY_READ_HOSTS: Partial<Record<ExchangeName, readonly string[]>> = {
  apexomni: ['omni.apex.exchange', 'testnet.omni.apex.exchange'],
  backpack: ['api.backpack.exchange'],
  bingx: ['open-api.bingx.com'],
  bingxspot: ['open-api.bingx.com'],
  bitget: ['api.bitget.com'],
  bitgetinverse: ['api.bitget.com'],
  bitgetusdc: ['api.bitget.com'],
  blofin: ['openapi.blofin.com'],
  // Linear, spot and inverse share one REST host; demo and testnet keep the exchange proxy.
  bybit: ['api.bybit.com'],
  bybitinverse: ['api.bybit.com'],
  bybitspot: ['api.bybit.com'],
  coinbase: ['api.coinbase.com', 'api.international.coinbase.com'],
  coinbasespot: ['api.coinbase.com', 'api.international.coinbase.com'],
  grvt: [
    'edge.grvt.io',
    'edge.testnet.grvt.io',
    'market-data.grvt.io',
    'market-data.testnet.grvt.io',
    'trades.grvt.io',
    'trades.testnet.grvt.io',
  ],
  mexc: ['api.mexc.com'],
  phemex: ['api.phemex.com', 'testnet-api.phemex.com'],
};

const hostOf = (url: string): string | undefined => {
  try {
    return new URL(url).host.toLowerCase();
  } catch {
    return undefined;
  }
};

/** Whether `exchange` opted in and `url` targets one of its public-proxy hosts. */
export const readsViaPublicProxy = (exchange: ExchangeName, url: string): boolean => {
  const host = hostOf(url);
  return host !== undefined && (PUBLIC_PROXY_READ_HOSTS[exchange]?.includes(host) ?? false);
};
