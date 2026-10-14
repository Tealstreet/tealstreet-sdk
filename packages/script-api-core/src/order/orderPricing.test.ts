import type { ExchangeName, Market } from '@tealstreet/safe-cex/src/types';

import { describe, expect, it } from 'vitest';

import {
  assertMinimumPrice,
  roundMarketCloseSize,
  roundOrderPrice,
  roundOrderSize,
  roundOrderSizeToNearest,
  venueAutoDownsizesReduceOrders,
  venueSupportsInfinityStops,
} from './orderPricing';

const market = (over: Partial<Market> & { isSpot?: boolean } = {}): Market =>
  ({
    symbol: 'TEST',
    isSpot: false,
    precision: { price: 0.01, amount: 0.1 },
    ...over,
  }) as unknown as Market;

describe('roundOrderSize', () => {
  it('rounds to nearest on a perp', () => {
    expect(roundOrderSize(1.06, market(), 'hyperliquid')).toBeCloseTo(1.1, 8);
    expect(roundOrderSize(1.04, market(), 'hyperliquid')).toBeCloseTo(1.0, 8);
  });

  // The venue AND market halves of shouldRoundSizeDown are both load-bearing:
  // one `hyperliquid` connection serves spot and perps.
  it('rounds DOWN only for hyperliquid spot', () => {
    expect(roundOrderSize(1.09, market({ isSpot: true }), 'hyperliquid')).toBeCloseTo(1.0, 8);
    expect(roundOrderSize(1.09, market({ isSpot: true }), 'hyperliquidspot')).toBeCloseTo(1.0, 8);
  });

  it('keeps hyperliquid PERPS on round-to-nearest', () => {
    expect(roundOrderSize(1.09, market({ isSpot: false }), 'hyperliquid')).toBeCloseTo(1.1, 8);
  });

  it('keeps other spot venues on round-to-nearest', () => {
    for (const venue of ['coinbase', 'backpack', 'kucoin', 'bybitspot']) {
      expect(roundOrderSize(1.09, market({ isSpot: true }), venue)).toBeCloseTo(1.1, 8);
    }
  });

  it('falls back to the default amount precision when the market has none', () => {
    const m = market({ precision: { price: 0.01, amount: 0 } } as Partial<Market>);
    // 10dp, not 8: at 8dp a no-op passes too, since the input sits 1e-9 away.
    // A missing fallback yields NaN (roundToNearestMultiple(x, 0)).
    expect(roundOrderSize(1.123456789, m, 'bybit')).toBeCloseTo(1.12345679, 10);
  });

  it('rounds to nearest when the exchange name is unknown', () => {
    expect(roundOrderSize(1.09, market({ isSpot: true }), undefined)).toBeCloseTo(1.1, 8);
  });
});

describe('roundOrderPrice', () => {
  it('rounds to the price precision', () => {
    expect(roundOrderPrice(100.567, market())).toBeCloseTo(100.57, 8);
  });

  it('falls back to the default price precision when the market has none', () => {
    const m = market({ precision: { price: 0, amount: 0.1 } } as Partial<Market>);
    expect(roundOrderPrice(100.567, m)).toBeCloseTo(100.57, 8);
  });
});

describe('roundOrderSizeToNearest', () => {
  // updateOrder must NOT inherit the placement round-down rule: truncating an
  // amend would silently shrink a resting order on hyperliquid spot.
  it('rounds to nearest even on hyperliquid spot', () => {
    expect(roundOrderSizeToNearest(1.09, market({ isSpot: true }))).toBeCloseTo(1.1, 8);
  });

  it('differs from roundOrderSize on exactly the round-down venues', () => {
    const spot = market({ isSpot: true });
    expect(roundOrderSize(1.09, spot, 'hyperliquid')).toBeCloseTo(1.0, 8);
    expect(roundOrderSizeToNearest(1.09, spot)).toBeCloseTo(1.1, 8);
  });
});

describe('roundMarketCloseSize', () => {
  it('rounds to nearest, ignoring the spot round-down rule', () => {
    expect(roundMarketCloseSize(1.09, market({ isSpot: true }))).toBeCloseTo(1.1, 8);
  });

  // Mirrors createMarketOrder: a residual position smaller than one lot step
  // must still be closable rather than collapsing to a zero-size order.
  it('falls back to the raw size when rounding collapses to zero', () => {
    expect(roundMarketCloseSize(0.04, market({ precision: { price: 0.01, amount: 0.1 } } as Partial<Market>))).toBe(
      0.04,
    );
  });
});

describe('venueAutoDownsizesReduceOrders', () => {
  it('is true on the venues that auto-downsize', () => {
    expect(venueAutoDownsizesReduceOrders('hyperliquid' as ExchangeName)).toBe(true);
  });

  it('is false on a venue that does not', () => {
    expect(venueAutoDownsizesReduceOrders('binance' as ExchangeName)).toBe(false);
  });

  it('answers false for an UNKNOWN venue rather than throwing', () => {
    // It runs on every reduce-only write, so throwing here failed the write
    // instead of skipping an optimisation. Note this is the opposite default
    // from `venueSupportsInfinityStops`, which must refuse what it cannot
    // confirm — see the comment on both.
    expect(() => venueAutoDownsizesReduceOrders('not-a-venue' as ExchangeName)).not.toThrow();
    expect(venueAutoDownsizesReduceOrders('not-a-venue' as ExchangeName)).toBe(false);
    expect(() => venueSupportsInfinityStops('not-a-venue' as ExchangeName)).toThrow();
  });
});

describe('assertMinimumPrice', () => {
  it('refuses a price under the venue minimum and names it', () => {
    const m = market({ limits: { price: { min: 5 } } } as Partial<Market>);
    expect(() => assertMinimumPrice(4, m)).toThrow('below the minimum 5');
    expect(() => assertMinimumPrice(5, m)).not.toThrow();
  });

  it('passes any price on a market with no minimum', () => {
    expect(() => assertMinimumPrice(0.0001, market({ limits: {} } as Partial<Market>))).not.toThrow();
  });
});
