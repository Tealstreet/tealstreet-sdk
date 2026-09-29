import { describe, expect, it } from 'vitest';

import type { Market } from '../types';
import { classifyMarket, isNonCryptoInstrument, venuesWithInstrumentClassFn } from './resolve';

/**
 * Payloads below are the real `/v5/market/instruments-info` rows, trimmed to the
 * fields this module reads, captured from the live linear category 2026-09-29.
 */
const market = (info: Record<string, unknown>): Market =>
  ({ id: String(info.symbol), symbol: String(info.symbol), base: '', quote: '', active: true, info }) as never;

const SHOP = market({
  symbol: 'SHOPUSDT',
  symbolType: 'stock',
  marketRegion: 'US',
  underlyingTicker: 'SHOP',
  fullName: 'Shopify',
});
const BTC = market({ symbol: 'BTCUSDT', symbolType: '', marketRegion: '', underlyingTicker: '', fullName: 'Bitcoin' });
const XAU = market({ symbol: 'XAUUSDT', symbolType: 'commodity', marketRegion: '', underlyingTicker: '' });
const ARKK = market({ symbol: 'ARKKUSDT', symbolType: 'ETF', marketRegion: 'US', underlyingTicker: 'ARKK' });
const EURUSD = market({ symbol: 'EURUSDUSDT', symbolType: 'forex', marketRegion: '', underlyingTicker: '' });
const AEON = market({ symbol: 'AEONUSDT', symbolType: 'innovation', marketRegion: '', underlyingTicker: '' });

describe('bybit instrument classification', () => {
  it('names a tokenized equity, with the real-world ticker behind it', () => {
    const result = classifyMarket('bybit', SHOP);
    expect(result).toEqual({
      kind: 'classified',
      assetClass: 'equity',
      venueLabel: 'stock',
      underlyingTicker: 'SHOP',
      marketRegion: 'US',
    });
  });

  it('treats an empty symbolType as crypto, which is how every perp is published', () => {
    const result = classifyMarket('bybit', BTC);
    expect(result).toMatchObject({ kind: 'classified', assetClass: 'crypto', venueLabel: '' });
    expect(isNonCryptoInstrument(result)).toBe(false);
  });

  /**
   * The trap. `innovation` is Bybit's innovation zone — new or higher-risk
   * TOKENS — so the obvious "non-empty symbolType means TradFi" rule mislabels
   * 127 of 886 linear markets and would warn on ordinary altcoins.
   */
  it('classifies the innovation zone as CRYPTO, not as a tradfi instrument', () => {
    const result = classifyMarket('bybit', AEON);
    expect(result).toMatchObject({ kind: 'classified', assetClass: 'crypto', venueLabel: 'innovation' });
    expect(isNonCryptoInstrument(result)).toBe(false);
  });

  /**
   * `marketRegion` is populated on exactly the stock and ETF rows, so a
   * region-based rule would silently miss metals and FX.
   */
  it('catches commodity and fx, which carry no marketRegion', () => {
    expect(classifyMarket('bybit', XAU)).toMatchObject({ assetClass: 'commodity', venueLabel: 'commodity' });
    expect(classifyMarket('bybit', EURUSD)).toMatchObject({ assetClass: 'fx', venueLabel: 'forex' });
    expect(isNonCryptoInstrument(classifyMarket('bybit', XAU))).toBe(true);
    expect(isNonCryptoInstrument(classifyMarket('bybit', EURUSD))).toBe(true);
  });

  it('classifies an ETF', () => {
    expect(classifyMarket('bybit', ARKK)).toMatchObject({ assetClass: 'etf', underlyingTicker: 'ARKK' });
  });

  it('applies to every bybit variant, including the demo account that reported this', () => {
    for (const venue of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot'] as const) {
      expect(classifyMarket(venue, SHOP)).toMatchObject({ assetClass: 'equity' });
    }
    expect(venuesWithInstrumentClassFn()).toHaveLength(4);
  });
});

describe('what we refuse to guess', () => {
  it('reports an unrecognised symbolType verbatim instead of assuming crypto', () => {
    const result = classifyMarket('bybit', market({ symbol: 'XUSDT', symbolType: 'prediction' }));
    expect(result).toEqual({ kind: 'unknown', reason: 'unrecognised bybit symbolType "prediction"' });
    expect(isNonCryptoInstrument(result)).toBeUndefined();
  });

  // Absent and empty are different claims: a cached market predating the field
  // says nothing, while an empty string is Bybit saying "no special type".
  it('separates a missing symbolType from an empty one', () => {
    expect(classifyMarket('bybit', market({ symbol: 'XUSDT' }))).toEqual({
      kind: 'unknown',
      reason: 'bybit instrument has no symbolType field',
    });
  });

  it('reports a market with no instruments-info payload', () => {
    expect(classifyMarket('bybit', { symbol: 'XUSDT' } as never)).toMatchObject({ kind: 'unknown' });
  });

  /**
   * Hyperliquid's `meta` universe carries name, szDecimals, maxLeverage and
   * marginTableId — nothing about what an asset IS. "HL lists only crypto" is a
   * claim about their roadmap, not something their API says, so it must not be
   * answered as crypto.
   */
  it('says we cannot tell on a venue with no classifier, rather than defaulting to crypto', () => {
    const result = classifyMarket('hyperliquid', market({ symbol: 'BTC' }));
    expect(result).toEqual({
      kind: 'unknown',
      reason: 'no instrument classifier implemented for hyperliquid',
    });
    expect(isNonCryptoInstrument(result)).toBeUndefined();
  });
});
