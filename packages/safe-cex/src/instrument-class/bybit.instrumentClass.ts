import type { Market } from '../types';
import type { AssetClass, InstrumentClassResult, VenueInstrumentClassFn } from './types';

/**
 * Bybit publishes `symbolType` on `/v5/market/instruments-info`, and
 * `fetchMarketsHelper` preserves the whole instrument object as `Market.info`,
 * so this costs no request.
 *
 * Measured against the live linear category on 2026-09-29, 886 instruments:
 *
 *     <empty>      502    BTCUSDT, SOLUSDT, ...
 *     stock        196    AAPLUSDT, SHOPUSDT, ...
 *     innovation   127    AEONUSDT, AGIUSDT, 1000TAGUSDT, ...
 *     ETF           54    ARKKUSDT, BITOUSDT, EWJUSDT, ...
 *     commodity      4    XAUUSDT, XAGUSDT, CLUSDT, BZUSDT
 *     forex          3    EURUSDUSDT, GBPUSDUSDT, USDJPYUSDT
 *
 * All 26 inverse instruments are empty. Empty means crypto.
 *
 * **`innovation` is CRYPTO, and this is the trap the whole file exists to
 * avoid.** It is Bybit's innovation zone — new or higher-risk TOKENS — so the
 * obvious rule "a non-empty symbolType means TradFi" mislabels 127 crypto
 * markets, one in seven of the category. Anything driving a warning off that
 * would cry wolf on ordinary altcoins.
 *
 * `marketRegion` cannot stand in for it either: it is populated on exactly the
 * stock and ETF rows (219 US + 21 HK + 8 KR + 2 CN = 250 = 196 + 54) and is
 * empty on commodity and forex, so keying off it silently misses gold and FX.
 */

/** Bybit's `symbolType` -> what the instrument is a claim on. */
const SYMBOL_TYPE_TO_ASSET_CLASS: Readonly<Record<string, AssetClass>> = {
  '': 'crypto',
  // Bybit's innovation zone. Still a token; see the note above.
  innovation: 'crypto',
  stock: 'equity',
  ETF: 'etf',
  commodity: 'commodity',
  forex: 'fx',
};

const text = (value: unknown): string | undefined => {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed === '' ? undefined : trimmed;
};

export const bybitInstrumentClass: VenueInstrumentClassFn = (market: Market): InstrumentClassResult => {
  const info = market.info;
  if (!info || typeof info !== 'object') {
    return { kind: 'unknown', reason: 'bybit market carries no instruments-info payload' };
  }

  const rawSymbolType = (info as { symbolType?: unknown }).symbolType;
  // Absent and empty are different: an older cached market predating the field
  // cannot be read as crypto, while an empty string is Bybit positively saying
  // "no special type", which is how every crypto perp is published.
  if (rawSymbolType === undefined || rawSymbolType === null) {
    return { kind: 'unknown', reason: 'bybit instrument has no symbolType field' };
  }
  if (typeof rawSymbolType !== 'string') {
    return { kind: 'unknown', reason: `bybit symbolType is ${typeof rawSymbolType}, not a string` };
  }

  const venueLabel = rawSymbolType.trim();
  const assetClass = SYMBOL_TYPE_TO_ASSET_CLASS[venueLabel];
  if (!assetClass) {
    // A value Bybit added since this table was written. Reporting it verbatim
    // beats guessing: a new tradfi type guessed as crypto is the exact failure
    // this module exists to prevent.
    return { kind: 'unknown', reason: `unrecognised bybit symbolType "${venueLabel}"` };
  }

  return {
    kind: 'classified',
    assetClass,
    venueLabel,
    underlyingTicker: text((info as { underlyingTicker?: unknown }).underlyingTicker),
    marketRegion: text((info as { marketRegion?: unknown }).marketRegion),
  };
};
