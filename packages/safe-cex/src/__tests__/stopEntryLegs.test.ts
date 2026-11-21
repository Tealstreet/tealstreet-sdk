import type { PlaceOrderOpts } from '../types';

import { describe, expect, it } from 'vitest';

import { OrderSide, OrderType } from '../types';
import { stopEntryLegsRefusal, stopEntryWithAttachedLegs } from './reference/stopEntryLegs';

// AGT-115: the TS reference refuses the same shapes as the Rust engine helper.
describe('stopEntryWithAttachedLegs', () => {
  const order = (type: OrderType, legs: Partial<PlaceOrderOpts>) =>
    ({ symbol: 'BTCUSDT', type, side: OrderSide.Buy, amount: 1, ...legs }) as PlaceOrderOpts;

  it('flags only a non-plain entry with a non-zero leg', () => {
    for (const type of [OrderType.Conditional, OrderType.StopLoss, OrderType.TakeProfit, OrderType.TrailingStopLoss]) {
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 1 }))).toBe(true);
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 0, takeProfit: 2 }))).toBe(true);
      expect(stopEntryWithAttachedLegs(order(type, {}))).toBe(false);
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 0, takeProfit: 0 }))).toBe(false);
    }
    for (const type of [OrderType.Limit, OrderType.Market]) {
      expect(stopEntryWithAttachedLegs(order(type, { stopLoss: 1, takeProfit: 2 }))).toBe(false);
    }
  });

  it('names the venue and says nothing was sent', () => {
    expect(stopEntryLegsRefusal('OKX')).toBe(
      "Attaching a stop-loss or take-profit to a stop entry isn't supported on OKX, so nothing was sent. Place the stop-loss after the entry fills.",
    );
  });
});
