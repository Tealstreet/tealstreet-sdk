import { roundToNearestMultiple } from '@tealstreet/math/src/roundToNearestMultiple';

import { roundCloseSize, roundMarketOrderSize, roundToStep } from './rounding';

describe('roundToStep', () => {
  it('is the host rounding, not a copy of it', () => {
    expect(roundToStep(1.23456789, 0.001)).toBe(roundToNearestMultiple(1.23456789, 0.001));
    expect(roundToStep(0.30000000000000004, 0.01)).toBe(0.3);
  });

  it('passes the value through when the market reports no step', () => {
    // `market.precision.amount` is 0 on a venue that never reported one. The
    // host falls back to a default there; refusing would be worse than
    // previewing the unrounded size the venue will itself round.
    expect(roundToStep(1.23456789, 0)).toBe(1.23456789);
  });
});

describe('roundCloseSize', () => {
  it('rounds to the lot step like any other size', () => {
    expect(roundCloseSize(1.2349, 0.001)).toBe(1.235);
  });

  it('keeps a residual smaller than one lot step closable', () => {
    // Rounding to nearest gives 0, and a zero-size close reads as the WHOLE
    // position on venues that support infinity stops.
    expect(roundCloseSize(0.0004, 0.001)).toBe(0.0004);
  });
});

describe('roundMarketOrderSize', () => {
  it('rounds to the lot step', () => {
    expect(roundMarketOrderSize(1.2349, 0.001)).toBe(1.235);
  });

  it('keeps the raw size on a REDUCE-ONLY order that collapses to zero', () => {
    expect(roundMarketOrderSize(0.0004, 0.001, { reduceOnly: true })).toBe(0.0004);
  });

  it('returns zero on a NON-reduce order that collapses, so the caller refuses it', () => {
    // This is the divergence the function exists to end: `createMarketOrder`
    // gated the fallback on reduce-only, `roundMarketCloseSize` did not, and
    // the same residual sized differently depending on the door it came in.
    expect(roundMarketOrderSize(0.0004, 0.001)).toBe(0);
    expect(roundMarketOrderSize(0.0004, 0.001, { reduceOnly: false })).toBe(0);
  });

  it('is exactly what roundCloseSize is', () => {
    for (const value of [1.2349, 0.0004, 0, 12_345.678]) {
      expect(roundMarketOrderSize(value, 0.001, { reduceOnly: true })).toBe(roundCloseSize(value, 0.001));
    }
  });
});
