import type { ExchangeName, Market } from '../types';

/**
 * What a listed contract is actually a claim on.
 *
 * ---------------------------------------------------------------------------
 * WHY THIS EXISTS
 * ---------------------------------------------------------------------------
 *
 * Venues now list tokenized equities, ETFs, metals and FX pairs beside crypto
 * perps, on the same endpoints, with the same symbol shape — `SHOPUSDT` looks
 * exactly like `SOLUSDT`. They do NOT trade the same: a user hits agreements
 * they have not signed, sessions that are closed on a weekend, and venue-side
 * rules that apply to nothing else. The first report of this was a user who
 * placed an order on Bybit's tokenized Shopify perp and got back "You must sign
 * the required agreement before trading this contract" with no way to know in
 * advance (user report, 2026-09-28).
 *
 * ---------------------------------------------------------------------------
 * WHAT THIS DOES NOT ANSWER
 * ---------------------------------------------------------------------------
 *
 * **This says what an instrument IS. It never says whether you can trade it.**
 * Eligibility is the venue's own product of instrument metadata, trading-session
 * calendars and per-account agreement state, and only the first of those is
 * public. So the honest sentence a caller may build from this is "this is a
 * tokenized equity, which may require an agreement you have not signed" — never
 * "you can trade this". Bybit's `/v5/user/agreement` SIGNS legal terms and is
 * not a status endpoint; nothing here may call it.
 */
export type AssetClass =
  /** A cryptocurrency or token. The default state of every venue we integrate. */
  | 'crypto'
  /** A single-name equity: Bybit `stock`, Binance's single-name TradFi perps. */
  | 'equity'
  /** A fund or ETF tracking something else. */
  | 'etf'
  /** Metals, energy — Bybit `commodity` (XAU, XAG, CL, BZ). */
  | 'commodity'
  /** A currency pair — Bybit `forex` (EURUSD, GBPUSD, USDJPY). */
  | 'fx'
  /** A basket or index that is none of the above. */
  | 'index';

/**
 * **`unknown` is not `crypto`, and keeping them apart is the point.**
 *
 * A venue that publishes no classification must be loudly unclassified.
 * Defaulting it to crypto states that an instrument is safe to trade like a
 * perp, which is indistinguishable from one we actually checked — and the whole
 * reason this module exists is that the two look identical from the symbol.
 * Hyperliquid is the live example: its `meta` universe carries only `name`,
 * `szDecimals`, `maxLeverage` and `marginTableId`, so "HL lists only crypto" is
 * an assertion about the venue's roadmap, not something its API says.
 */
export type InstrumentClassResult =
  | {
      readonly kind: 'classified';
      readonly assetClass: AssetClass;
      /** The venue's own value, verbatim, so a surprise is visible rather than mapped away. */
      readonly venueLabel: string;
      /** The real-world ticker behind a tokenized instrument, where the venue gives one. */
      readonly underlyingTicker?: string;
      /** Listing venue region (Bybit `marketRegion`: US, HK, KR, CN). */
      readonly marketRegion?: string;
    }
  /** We cannot say. Never render as crypto; never use it to claim something is tradeable. */
  | { readonly kind: 'unknown'; readonly reason: string };

/**
 * Classification is derived from the MARKET alone — `Market.info` holds the
 * venue's raw instrument object, so no venue needs a request of its own and
 * none needs per-venue inputs. That is the whole reason this registry is
 * simpler than revenue attribution, which has to carry commercial facts the fill cannot
 * supply.
 */
export type VenueInstrumentClassFn = (market: Market) => InstrumentClassResult;

/**
 * Venues with a written classifier. Adding a name here without registering its
 * function is a COMPILE error, so wiring one up is not something you can forget.
 */
export type InstrumentClassVenue = 'bybit' | 'bybitdemo' | 'bybitinverse' | 'bybitspot';

export type InstrumentClassRegistry = { readonly [K in InstrumentClassVenue]: VenueInstrumentClassFn };

export const isInstrumentClassVenue = (exchange: ExchangeName): exchange is InstrumentClassVenue =>
  exchange === 'bybit' || exchange === 'bybitdemo' || exchange === 'bybitinverse' || exchange === 'bybitspot';
