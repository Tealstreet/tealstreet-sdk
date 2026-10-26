// The Rust-only beta bundle's eligibility table (docs/plans/RUST_BETA_BUNDLE.md),
// the single source for its family build list, its generated web loader
// (engine.beta.web.mjs) and its venue check. Every other Rust venue is OUT and
// refused by name before any wasm is fetched.
import { venueFamily } from './venues.mjs';

const betaVenues = Object.freeze({
  apexomni: { family: 'apexomni' },
  asterdex: { family: 'asterdex' },
  binance: { family: 'binance' },
  binanceinverse: { family: 'binance' },
  binancepm: { family: 'binance' },
  binancespot: { family: 'binance' },
  bingx: { family: 'bingx' },
  bitget: { family: 'bitget' },
  bitgetusdc: { family: 'bitget' },
  bitunix: { family: 'bitunix' },
  blofin: { family: 'blofin' },
  bybit: { family: 'bybit' },
  bybitdemo: { family: 'bybit' },
  bybitinverse: { family: 'bybit' },
  bybitspot: { family: 'bybit' },
  cryptocom: { family: 'cryptocom' },
  hyperliquid: { family: 'hyperliquid' },
  hyperliquidspot: { family: 'hyperliquid' },
  kraken: { family: 'kraken' },
  krakenspot: { family: 'kraken' },
  lighter: { family: 'lighter' },
  okx: { family: 'okx' },
  okxdemo: { family: 'okx' },
  okxeu: { family: 'okx' },
  okxeudemo: { family: 'okx' },
  // Local key only: remote or hosted Paradex signer shapes are not in the beta.
  paradex: { family: 'paradex', signers: ['local'] },
  phemex: { family: 'phemex' },
});

export const betaVenueNames = Object.freeze(Object.keys(betaVenues).sort());

export const betaVenueFamilies = Object.freeze([...new Set(Object.values(betaVenues).map((venue) => venue.family))].sort());

export class BetaVenueUnavailableError extends Error {
  constructor(venue) {
    super(`${venue} is not available in the beta bundle`);
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

/** The signer shapes a beta venue accepts, or `undefined` when its table entry sets no restriction. */
export function betaVenueSigners(venue) {
  return Object.hasOwn(betaVenues, venue) ? betaVenues[venue].signers : undefined;
}
