import { describe, expect, it } from 'vitest';

import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { breakevenPrice } from './breakevenPrice';

const position = (over: Record<string, unknown> = {}) =>
  ({ entryPrice: 100, side: PositionSide.Long, contracts: 10, ...over }) as never;

describe('breakevenPrice', () => {
  it('is the entry price when nothing has been banked', () => {
    expect(breakevenPrice(position())).toBe(100);
    expect(breakevenPrice(position({ realizedPnl: 0 }))).toBe(100);
  });

  // Banked profit lets a long exit LOWER and still be flat.
  it('moves a long DOWN by the profit already banked', () => {
    expect(breakevenPrice(position({ realizedPnl: 50 }))).toBe(95);
  });

  // The same profit moves a short UP. The direction flips with the side, which
  // is the whole reason this takes the side rather than a signed size.
  it('moves a short UP by the same amount', () => {
    expect(breakevenPrice(position({ side: PositionSide.Short, realizedPnl: 50 }))).toBe(105);
  });

  it('moves the other way on a banked loss', () => {
    expect(breakevenPrice(position({ realizedPnl: -50 }))).toBe(105);
    expect(breakevenPrice(position({ side: PositionSide.Short, realizedPnl: -50 }))).toBe(95);
  });

  it('reads size as a magnitude, so a venue reporting a signed short agrees', () => {
    expect(breakevenPrice(position({ side: PositionSide.Short, contracts: -10, realizedPnl: 50 }))).toBe(105);
  });

  it('falls back to entry rather than dividing by zero or returning NaN', () => {
    expect(breakevenPrice(position({ contracts: 0, realizedPnl: 50 }))).toBe(100);
    expect(breakevenPrice(position({ realizedPnl: NaN }))).toBe(100);
    expect(breakevenPrice(position({ realizedPnl: Infinity }))).toBe(100);
  });
});

/**
 * An inverse contract settles in the BASE currency and its PnL is reciprocal in
 * price, so the linear formula adds a base-denominated quantity to a
 * quote-denominated price. `useOpenPositionsColumns` branches the same way for
 * its worst-case-loss column.
 */
describe('breakevenPrice on inverse contracts', () => {
  const inverse = { isInverse: true };

  // Long 10 @ 100 with 0.05 base banked: 1/P = 1/100 + 0.05/10 = 0.015, P = 66.67.
  it('works in reciprocal space, not by adding to the price', () => {
    const p = breakevenPrice(position({ realizedPnl: 0.05 }), inverse);
    expect(p).toBeCloseTo(66.667, 3);
    // The linear formula would have said 99.995 — close enough to entry to look
    // plausible and be wrong by a third of the price.
    expect(p).not.toBeCloseTo(99.995, 3);
  });

  // Direction must still match linear: banked profit lets a long exit LOWER.
  it('moves a long DOWN and a short UP, the same directions as linear', () => {
    expect(breakevenPrice(position({ realizedPnl: 0.05 }), inverse)).toBeLessThan(100);
    expect(breakevenPrice(position({ side: PositionSide.Short, realizedPnl: 0.05 }), inverse)).toBeGreaterThan(100);
  });

  it('falls back to entry when a banked loss has no breakeven price', () => {
    // 1/100 - 0.5/10 is negative: no price closes this flat.
    expect(breakevenPrice(position({ side: PositionSide.Short, realizedPnl: 0.5 }), inverse)).toBe(100);
  });

  it('is unchanged from linear when the flag is absent', () => {
    expect(breakevenPrice(position({ realizedPnl: 50 }))).toBe(95);
  });
});
