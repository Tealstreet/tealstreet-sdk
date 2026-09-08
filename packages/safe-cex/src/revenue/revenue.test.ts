import type { ExchangeName, Execution, Market } from '../types';

import { describe, expect, it } from 'vitest';

import { HYPERLIQUID_BUILDER_RATE, hyperliquidRevenue, verifyAgainstVenue } from './hyperliquid.revenue';
import { isRevenueVenue, resolveVenueRevenue, revenueForUnknownVenue, venuesWithRevenueFn } from './resolve';

const fill = (over: Partial<Execution> = {}): Execution =>
  ({
    timestamp: 1_788_560_501,
    symbol: 'BTC-PERP',
    price: 79_654,
    qty: 0.13,
    side: 'sell',
    commission: 0.005508,
    commissionAsset: 'USDC',
    maker: false,
    ...over,
  }) as Execution;

const market = (over: Partial<Market> = {}): Market => ({ isSpot: false, ...over }) as Market;

const evaluate = (over: Partial<Execution> = {}, m: Partial<Market> = {}, builderFeeTenthsBps = 10) =>
  hyperliquidRevenue({ fill: fill(over), market: market(m), at: Date.now(), inputs: { builderFeeTenthsBps } });

describe('hyperliquidRevenue', () => {
  it('is notional times the DEFAULT tier when that is what the fill was charged', () => {
    const result = evaluate({ price: 100_000, qty: 1 });

    expect(result).toEqual({ kind: 'earned', amount: 10, asset: 'USDC', basis: 'computed_from_notional' });
    expect(HYPERLIQUID_BUILDER_RATE).toBe(0.0001);
  });

  // The ladder an affiliate picks from. Same fill, same venue, same second —
  // three different answers, which is the whole reason the rate is an input
  // rather than a constant.
  it.each([
    [10, 100],
    [30, 300],
    [50, 500],
  ])('values a $1M fill at %i tenths of a bp as $%i', (tenths, expected) => {
    expect(evaluate({ price: 100_000, qty: 10 }, {}, tenths)).toMatchObject({ kind: 'earned', amount: expected });
  });

  it('says UNKNOWN rather than falling back to 1bp when the tier is unusable', () => {
    // The dangerous direction: defaulting here values a 5bp fill at a fifth of
    // what the venue actually paid us, and nothing but the daily reconciliation
    // would ever notice.
    expect(evaluate({ price: 100_000, qty: 1 }, {}, 0)).toMatchObject({ kind: 'unknown' });
    expect(evaluate({ price: 100_000, qty: 1 }, {}, NaN)).toMatchObject({ kind: 'unknown' });
  });

  it('earns nothing on spot, because the venue charges nothing there', () => {
    // Measured across all 233 published days: 7 spot fills, $44.23 notional,
    // $0.00 collected. Computing 1bp on those invents revenue never received.
    expect(evaluate({ price: 18.4, qty: 1 }, { isSpot: true })).toEqual({
      kind: 'none',
      reason: 'venue-pays-nothing',
    });
  });

  it('says UNKNOWN rather than zero when the notional is unusable', () => {
    expect(evaluate({ price: 0, qty: 0 })).toMatchObject({ kind: 'unknown' });
  });

  it('reports in the asset the fee was charged in', () => {
    expect(evaluate({ commissionAsset: 'USDT' })).toMatchObject({ asset: 'USDT' });
  });
});

describe('verifyAgainstVenue', () => {
  it('agrees when our arithmetic matches the venue, within the feed truncation', () => {
    // A real fill: 79654 * 0.00013 = $10.35482 notional -> $0.001035 at 1bp,
    // and the venue reported exactly 0.001035.
    const computed = evaluate({ price: 79_654, qty: 0.00013 });

    expect(verifyAgainstVenue(computed, 0.001035)).toEqual({ agrees: true });
  });

  it('catches a rate drift on a single fill', () => {
    const computed = evaluate({ price: 100_000, qty: 1 });

    expect(verifyAgainstVenue(computed, 5)).toMatchObject({ agrees: false });
  });

  it('flags the venue paying us while we computed nothing', () => {
    // The direction that silently under-pays an affiliate.
    const spot = evaluate({ price: 18.4, qty: 1 }, { isSpot: true });

    expect(verifyAgainstVenue(spot, 0.5)).toMatchObject({ agrees: false });
  });

  it('is silent when the venue reports nothing to compare against', () => {
    expect(verifyAgainstVenue(evaluate(), undefined)).toEqual({ agrees: true });
  });
});

describe('the registry', () => {
  it('resolves an implemented venue', () => {
    expect(resolveVenueRevenue('hyperliquid', { fill: fill(), market: market(), at: 0, inputs: { builderFeeTenthsBps: 10 } })).toMatchObject({
      kind: 'earned',
    });
  });

  it('narrows only venues that have a function', () => {
    expect(isRevenueVenue('hyperliquid' as ExchangeName)).toBe(true);
    expect(isRevenueVenue('bybit' as ExchangeName)).toBe(false);
  });

  it('reports an unimplemented venue as UNKNOWN, never as zero', () => {
    // We tag orders on twelve venues; eleven have no function yet. Rendering
    // those as $0 would state that an affiliate is owed nothing.
    const result = revenueForUnknownVenue('bybit' as ExchangeName);

    expect(result.kind).toBe('unknown');
    expect(result).toMatchObject({ reason: expect.stringContaining('bybit') });
  });

  it('lists what is actually implemented', () => {
    expect(venuesWithRevenueFn()).toEqual(['hyperliquid']);
  });
});
