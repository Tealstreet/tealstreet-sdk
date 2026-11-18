// The Rust-only beta bundle's eligibility table (docs/plans/RUST_BETA_BUNDLE.md),
// the single source for its family build list, its generated web loader
// (engine.beta.web.mjs) and its venue check. Every other Rust venue is OUT and
// refused by name before any wasm is fetched. Account-mode policy (local key vs
// remote or payload signer, Paradex local only) is not here: it is enforced at
// construction by safe-cex's assertBetaAccount (exchanges/rust/betaVenues.ts).
import { ExchangeError } from './host.mjs';
import { venueFamily } from './venues.mjs';

const betaVenues = Object.freeze({
  apexomni: { family: 'apexomni' },
  asterdex: { family: 'asterdex' },
  backpack: { family: 'backpack' },
  binance: { family: 'binance' },
  binanceinverse: { family: 'binance' },
  binanceoptions: { family: 'binance' },
  binancepm: { family: 'binance' },
  binancespot: { family: 'binance' },
  bingx: { family: 'bingx' },
  bitget: { family: 'bitget' },
  bitgetinverse: { family: 'bitget' },
  bitgetusdc: { family: 'bitget' },
  bitrue: { family: 'bitrue' },
  bitunix: { family: 'bitunix' },
  blofin: { family: 'blofin' },
  bybit: { family: 'bybit' },
  bybitdemo: { family: 'bybit' },
  bybitinverse: { family: 'bybit' },
  bybitspot: { family: 'bybit' },
  coinbase: { family: 'coinbase' },
  coinbasespot: { family: 'coinbase' },
  cryptocom: { family: 'cryptocom' },
  grvt: { family: 'grvt' },
  hyperliquid: { family: 'hyperliquid' },
  hyperliquidspot: { family: 'hyperliquid' },
  hyrotrader: { family: 'bybit' },
  kodiakfi: { family: 'orderly' },
  kraken: { family: 'kraken' },
  krakenspot: { family: 'kraken' },
  kucoin: { family: 'kucoin' },
  kucoinspot: { family: 'kucoinspot' },
  lighter: { family: 'lighter' },
  'lighter-robinhood': { family: 'lighter' },
  mexc: { family: 'mexc' },
  mode: { family: 'orderly' },
  okx: { family: 'okx' },
  okxdemo: { family: 'okx' },
  okxeu: { family: 'okx' },
  okxeudemo: { family: 'okx' },
  orderly: { family: 'orderly' },
  paradex: { family: 'paradex' },
  phemex: { family: 'phemex' },
  what: { family: 'orderly' },
  woo: { family: 'woo' },
  woofi: { family: 'orderly' },
});

export const betaVenueNames = Object.freeze(Object.keys(betaVenues).sort());

export const betaVenueFamilies = Object.freeze([...new Set(Object.values(betaVenues).map((venue) => venue.family))].sort());

export class BetaVenueUnavailableError extends ExchangeError {
  constructor(venue) {
    const message = `${venue} is not available in the beta bundle`;
    super(message, { message, kind: 'unsupported', outcome: 'notSent' });
    this.name = 'BetaVenueUnavailableError';
    this.venue = venue;
  }
}

/** The family of a beta venue; an unknown venue throws as `venueFamily` does, an OUT one throws `BetaVenueUnavailableError`. */
export function betaVenueFamily(venue) {
  const family = venueFamily(venue);
  if (!Object.hasOwn(betaVenues, venue)) throw new BetaVenueUnavailableError(venue);
  return family;
}
