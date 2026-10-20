/** Aliases share one engine binary; unsupported names fail before loading wasm. */
const families = Object.freeze({
  backpack: 'backpack',
  binance: 'binance',
  binancepm: 'binance',
  bingx: 'bingx',
  bitget: 'bitget',
  bitgetusdc: 'bitget',
  bitgetinverse: 'bitget',
  bitrue: 'bitrue',
  blofin: 'blofin',
  bybit: 'bybit',
  bybitinverse: 'bybit',
  bybitspot: 'bybit',
  bybitdemo: 'bybit',
  hyrotrader: 'bybit',
  cryptocom: 'cryptocom',
  hyperliquid: 'hyperliquid',
  lighter: 'lighter',
  mexc: 'mexc',
  orderly: 'orderly',
  'tealstreet-dex': 'orderly',
  kodiakfi: 'orderly',
  mode: 'orderly',
  salsadex: 'orderly',
  what: 'orderly',
  woofi: 'orderly',
  okx: 'okx',
  okxeu: 'okx',
  okxdemo: 'okx',
  okxeudemo: 'okx',
  weex: 'weex',
  woo: 'woo',
});

export function venueFamily(venue) {
  if (!Object.hasOwn(families, venue)) throw new Error(`unsupported Rust venue: ${venue}`);
  return families[venue];
}
