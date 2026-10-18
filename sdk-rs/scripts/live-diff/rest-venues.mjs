import { venueHosts } from './venues.mjs';

// Include every TS registry name, including spot/inverse/demo/broker variants.
// Origins are literals from the maintained venue configs. HyroTrader requires
// an operator-configured origin; do not invent one or forward arbitrary URLs.
export const restVenueHosts = {
  ...venueHosts,
  binanceinverse: ['dapi.binance.com'],
  binancepm: ['papi.binance.com', 'fapi.binance.com'],
  binancespot: ['api.binance.com'],
  bitgetinverse: ['api.bitget.com'],
  bitgetusdc: ['api.bitget.com'],
  bybitdemo: ['api.bybit.com', 'api-demo.bybit.com'],
  hyrotrader: [],
  bybitinverse: ['api.bybit.com'],
  bybitspot: ['api.bybit.com'],
  coinbasespot: ['api.coinbase.com', 'api.international.coinbase.com'],
  hyperliquidspot: ['api.hyperliquid.xyz'],
  krakenspot: ['api.kraken.com'],
  kucoinspot: ['api.kucoin.com'],
  'lighter-robinhood': ['mainnet.zklighter.elliot.ai'],
  okxdemo: ['openapi.okx.com', 'www.okx.com', 'aws.okx.com'],
  okxeu: ['eea.okx.com'],
  okxeudemo: ['eea.okx.com'],
  kodiakfi: ['api-evm.orderly.org'],
  mode: ['api-evm.orderly.org'],
  salsadex: ['api-evm.orderly.org'],
  'tealstreet-dex': ['api-evm.orderly.org'],
  what: ['api-evm.orderly.org'],
};
