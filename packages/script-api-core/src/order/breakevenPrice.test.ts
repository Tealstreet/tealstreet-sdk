import { describe, expect, it } from 'vitest';

import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { breakevenPrice } from './breakevenPrice';

const input = (over: Record<string, unknown> = {}) =>
  ({ entryPrice: 100, size: 10, side: PositionSide.Long, ...over }) as never;

describe('breakevenPrice', () => {
  it('is the entry price when nothing is banked and nothing paid', () => {
    expect(breakevenPrice(input())).toBe(100);
    expect(breakevenPrice(input({ bankedPnl: 0, fees: 0 }))).toBe(100);
  });

  /**
   * Banked profit lets a LONG exit lower and still be flat overall:
   * 50 + (P - 100) * 10 = 0 at P = 95. The web copy returned 105 here, because
   * it had no side branch and was the short answer applied to both.
   */
  it('moves a long DOWN by what it has banked', () => {
    expect(breakevenPrice(input({ bankedPnl: 50 }))).toBe(95);
  });

  it('moves a short UP by the same amount', () => {
    expect(breakevenPrice(input({ side: PositionSide.Short, bankedPnl: 50 }))).toBe(105);
  });

  it('moves the other way on a banked loss', () => {
    expect(breakevenPrice(input({ bankedPnl: -50 }))).toBe(105);
    expect(breakevenPrice(input({ side: PositionSide.Short, bankedPnl: -50 }))).toBe(95);
  });

  // Fees are a cost, so they move break-even AWAY from a profitable exit.
  it('puts a long that has only paid fees ABOVE its entry', () => {
    expect(breakevenPrice(input({ fees: 10 }))).toBe(101);
    expect(breakevenPrice(input({ side: PositionSide.Short, fees: 10 }))).toBe(99);
  });

  it('nets fees against banked profit', () => {
    expect(breakevenPrice(input({ bankedPnl: 50, fees: 10 }))).toBe(96);
  });

  /**
   * Callers disagree about whether size is signed — `processTrades` emits
   * `Math.max(totalBuy, totalSell)` while a live Position is unsigned with an
   * explicit side. Direction must come from `side` alone.
   */
  it('takes size as a magnitude, so a signed caller cannot flip the answer', () => {
    expect(breakevenPrice(input({ side: PositionSide.Short, size: -10, bankedPnl: 50 }))).toBe(105);
    expect(breakevenPrice(input({ size: -10, bankedPnl: 50 }))).toBe(95);
  });

  it('falls back to entry rather than dividing by zero or returning NaN', () => {
    expect(breakevenPrice(input({ size: 0, bankedPnl: 50 }))).toBe(100);
    expect(breakevenPrice(input({ bankedPnl: NaN }))).toBe(100);
    expect(breakevenPrice(input({ bankedPnl: Infinity }))).toBe(100);
  });

  /**
   * No `isInverse` input exists, on purpose: `contracts` is normalised to base
   * so `size x price-distance` holds for both, and the rest of the price layer
   * (`utils/risk.ts`, `calculateReturns`) contains no inverse branch either.
   */
  it('exposes no inverse switch to get wrong', () => {
    expect(Object.keys(input({ bankedPnl: 1 }))).not.toContain('isInverse');
  });
});
