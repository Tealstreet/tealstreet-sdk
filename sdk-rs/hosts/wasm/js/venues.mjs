/** Aliases share one engine binary; unsupported names fail before loading wasm. */
const families = Object.freeze({
  binance: 'binance',
  binancepm: 'binance',
  bybit: 'bybit',
  cryptocom: 'cryptocom',
  okx: 'okx',
  okxeu: 'okx',
  okxdemo: 'okx',
  okxeudemo: 'okx',
});

export function venueFamily(venue) {
  if (!Object.hasOwn(families, venue)) throw new Error(`unsupported Rust venue: ${venue}`);
  return families[venue];
}
