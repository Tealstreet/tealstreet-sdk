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
