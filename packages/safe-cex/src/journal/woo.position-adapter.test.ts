import { describe, expect, it } from 'vitest';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';

import { reconstructPositionIntervals } from './position-intervals';
import { wooOrdersToExecutions } from './woo.position-adapter';

/**
 * Fixtures transcribed from the live account (sheet row 28), not invented.
 *
 * WOO is the only venue here folded from the ORDER axis, so the cases that matter are the
 * ones where an order is not a fill: a cancelled order, a partial fill, and the difference
 * between the limit price and the executed price.
 */

const order = (over: Partial<any> = {}): any => ({
  id: '1',
  symbol: 'ETHUSDT',
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Closed,
  price: 1900,
  averagePrice: 1876.6,
  amount: 0.0108,
  filled: 0.0108,
  reduceOnly: false,
  positionSide: 'both',
  createdTime: 1786690690000,
  updatedTime: 1786690690847,
  realizedPnl: 0,
  fees: 0.00182406,
  feeAsset: 'USDT',
  ...over,
});

describe('wooOrdersToExecutions', () => {
  it('drops orders that never filled — 27.4% of this account, and they moved no size', () => {
    const executions = wooOrdersToExecutions([
      order({ id: 'filled' }),
      order({ id: 'cancelled', status: OrderStatus.Canceled, filled: 0 }),
    ]);
    expect(executions.map((e) => e.id)).toEqual(['filled']);
  });

  it('prices on averagePrice, NOT the limit price', () => {
    // Using `price` would value every fill where it was asked rather than where it traded.
    // The magnitude invariant would then read that as a PnL scale error on the whole venue.
    expect(wooOrdersToExecutions([order()])[0].price).toBe(1876.6);
  });

  it('converts ms to SECONDS once — the unit this venue has already got wrong twice', () => {
    expect(wooOrdersToExecutions([order()])[0].timestamp).toBe(Math.floor(1786690690847 / 1000));
  });

  it('keeps a genuine zero realizedPnl as 0, distinct from absent', () => {
    // A breakeven close is real. Folding it to `undefined` would make it indistinguishable
    // from "this route cannot say", which on the woo FILLS axes is the actual state.
    expect(wooOrdersToExecutions([order({ realizedPnl: 0 })])[0].realizedPnl).toBe(0);
    expect(wooOrdersToExecutions([order({ realizedPnl: undefined })])[0].realizedPnl).toBeUndefined();
  });

  it('does NOT negate the fee — measured positive-when-charged on this route', () => {
    expect(wooOrdersToExecutions([order()])[0].commission).toBe(0.00182406);
  });

  it('folds an open-then-close pair into one closed round trip', () => {
    const executions = wooOrdersToExecutions([
      order({ id: 'open', side: OrderSide.Buy, filled: 1, averagePrice: 100, realizedPnl: 0, updatedTime: 1_000_000 }),
      order({ id: 'close', side: OrderSide.Sell, filled: 1, averagePrice: 110, realizedPnl: 10, updatedTime: 2_000_000 }),
    ]);
    const intervals = reconstructPositionIntervals(executions);
    expect(intervals).toHaveLength(1);
    expect(intervals[0].closedAt).toBeDefined();
    expect(intervals[0].closedSize).toBe(1);
    expect(intervals[0].side).toBe(PositionSide.Long);
  });

  it('leaves a still-open position without a closedAt rather than inventing one', () => {
    const executions = wooOrdersToExecutions([order({ id: 'open', side: OrderSide.Buy, filled: 1 })]);
    const intervals = reconstructPositionIntervals(executions);
    expect(intervals[0]?.closedAt).toBeUndefined();
  });

  it('refuses an order with no usable executed price instead of pricing it at zero', () => {
    expect(wooOrdersToExecutions([order({ averagePrice: undefined })])).toHaveLength(0);
    expect(wooOrdersToExecutions([order({ averagePrice: 0 })])).toHaveLength(0);
  });
});
