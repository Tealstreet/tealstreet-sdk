// Every Rust venue name and the family build (a hosts/wasm Cargo feature)
// that contains it. Unknown names fail before any wasm is fetched.
import { ExchangeError } from './host.mjs';

const families = Object.freeze({
  apexomni: 'apexomni',
  asterdex: 'asterdex',
  backpack: 'backpack',
  binance: 'binance',
  binanceinverse: 'binance',
  binancepm: 'binance',
  binanceoptions: 'binance',
  binancespot: 'binance',
  bingx: 'bingx',
  bingxspot: 'bingx',
  bitget: 'bitget',
  bitgetinverse: 'bitget',
  bitgetusdc: 'bitget',
  bitrue: 'bitrue',
  bitunix: 'bitunix',
  blofin: 'blofin',
  bybit: 'bybit',
  bybitdemo: 'bybit',
  bybitinverse: 'bybit',
  bybitspot: 'bybit',
  hyrotrader: 'bybit',
  coinbase: 'coinbase',
  coinbasespot: 'coinbase',
  cryptocom: 'cryptocom',
  grvt: 'grvt',
  hyperliquid: 'hyperliquid',
  hyperliquidspot: 'hyperliquid',
  kraken: 'kraken',
  krakenspot: 'kraken',
  kucoin: 'kucoin',
  kucoinspot: 'kucoinspot',
  lighter: 'lighter',
  'lighter-robinhood': 'lighter',
  mexc: 'mexc',
  okx: 'okx',
  okxdemo: 'okx',
  okxeu: 'okx',
  okxeudemo: 'okx',
  orderly: 'orderly',
  kodiakfi: 'orderly',
  mode: 'orderly',
  what: 'orderly',
  woofi: 'orderly',
  paradex: 'paradex',
  phemex: 'phemex',
  polymarket: 'polymarket',
  txflow: 'txflow',
  weex: 'weex',
  woo: 'woo',
});

export const venueFamilies = Object.freeze([...new Set(Object.values(families))].sort());

export function venueFamily(venue) {
  if (!Object.hasOwn(families, venue)) {
    // Retired names (e.g. bitmex) stay on saved accounts: refuse typed, so one account fails alone.
    const message = `unsupported Rust venue: ${venue}`;
    throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
  }
  return families[venue];
}
