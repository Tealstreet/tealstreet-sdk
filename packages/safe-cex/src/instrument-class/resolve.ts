import type { ExchangeName, Market } from '../types';
import type { InstrumentClassResult, InstrumentClassRegistry, InstrumentClassVenue } from './types';

import { bybitInstrumentClass } from './bybit.instrumentClass';
import { isInstrumentClassVenue } from './types';

/**
 * One entry per venue whose classifier is written.
 *
 * The absent venues are not "all crypto", they are UNWRITTEN, and
 * `classifyMarket` says exactly that. Two are known to list non-crypto today
 * and are worth writing next:
 *
 *   * binance — `contractType: 'TRADIFI_PERPETUAL'` on `/fapi/v1/*`. The wire
 *     value carries Binance's own typo (an extra "I") and must not be
 *     "corrected"; see `exchanges/binance/CLAUDE.md`.
 *   * bingx — already has `isLikelyTradfiMarket`, a symbol-suffix heuristic
 *     used to preflight one-way-mode rejections. Folding it in here would give
 *     it a real result type instead of a boolean.
 *
 * Hyperliquid is the case that proves `unknown` must exist: its `meta` universe
 * publishes `name`, `szDecimals`, `maxLeverage` and `marginTableId` and nothing
 * about what an asset is, so there is no classifier to write from its API.
 */
export const VENUE_INSTRUMENT_CLASS: InstrumentClassRegistry = {
  bybit: bybitInstrumentClass,
  bybitdemo: bybitInstrumentClass,
  bybitinverse: bybitInstrumentClass,
  bybitspot: bybitInstrumentClass,
};

export const resolveVenueInstrumentClass = (exchange: InstrumentClassVenue, market: Market): InstrumentClassResult =>
  VENUE_INSTRUMENT_CLASS[exchange](market);

/**
 * The entry point for a caller holding a runtime `ExchangeName`. Returns
 * `unknown` rather than throwing: an unimplemented venue is a normal state of
 * this system, not an error, and it must be recorded as "we cannot say" instead
 * of defaulting to crypto.
 */
export const classifyMarket = (exchange: ExchangeName, market: Market): InstrumentClassResult =>
  isInstrumentClassVenue(exchange)
    ? resolveVenueInstrumentClass(exchange, market)
    : { kind: 'unknown', reason: `no instrument classifier implemented for ${exchange}` };

/**
 * Whether an instrument is something other than crypto.
 *
 * `undefined` is the third answer and callers must handle it: an unclassified
 * instrument is not known to be crypto. A boolean alone would collapse "we
 * checked, it is a perp" into "we have no idea", which is how a warning goes
 * missing on the venue that needed it.
 */
export const isNonCryptoInstrument = (result: InstrumentClassResult): boolean | undefined =>
  result.kind === 'classified' ? result.assetClass !== 'crypto' : undefined;

export const venuesWithInstrumentClassFn = (): InstrumentClassVenue[] =>
  Object.keys(VENUE_INSTRUMENT_CLASS) as InstrumentClassVenue[];
