/** Aliases share one engine binary; unsupported names fail before loading wasm. */
const families = Object.freeze({
  backpack: 'backpack',
  binance: 'binance',
  binancepm: 'binance',
  bingx: 'bingx',
  blofin: 'blofin',
  bybit: 'bybit',
  cryptocom: 'cryptocom',
  mexc: 'mexc',
  okx: 'okx',
  okxeu: 'okx',
  okxdemo: 'okx',
  okxeudemo: 'okx',
  weex: 'weex',
});

export function venueFamily(venue) {
  if (!Object.hasOwn(families, venue)) throw new Error(`unsupported Rust venue: ${venue}`);
  return families[venue];
}
