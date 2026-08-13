import { describe, expect, it } from 'vitest';

import { getCandleCacheKey } from '../candleCache/CandleCache';

describe('getCandleCacheKey testnet namespacing', () => {
  it('keeps the existing shape for live, so no cached entry is invalidated', () => {
    expect(getCandleCacheKey('bitmex' as any, 'XBTUSD', '1m' as any)).toBe('bitmex-XBTUSD-1m');
    expect(getCandleCacheKey('bitmex' as any, 'XBTUSD', '1m' as any, false)).toBe('bitmex-XBTUSD-1m');
  });

  it('separates testnet from live on the same ExchangeName', () => {
    // The whole point: testnet is a per-account flag, not a separate exchange,
    // so without this both environments share one cache entry and the chart
    // can render the wrong market's history.
    const live = getCandleCacheKey('bitmex' as any, 'XBTUSD', '1m' as any, false);
    const test = getCandleCacheKey('bitmex' as any, 'XBTUSD', '1m' as any, true);
    expect(test).not.toBe(live);
    expect(test).toBe('bitmex-testnet-XBTUSD-1m');
  });

  it('applies to every venue offering the testnet checkbox, not just bitmex', () => {
    for (const ex of ['binance', 'binancepm', 'hyperliquid', 'grvt', 'paradex', 'asterdex']) {
      expect(getCandleCacheKey(ex as any, 'BTCUSDT', '1m' as any, true)).not.toBe(
        getCandleCacheKey(ex as any, 'BTCUSDT', '1m' as any, false),
      );
    }
  });
});
