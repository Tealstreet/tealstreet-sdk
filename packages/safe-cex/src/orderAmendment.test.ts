import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from './exchangeMetadata';
import { getOrderAmendmentError } from './orderAmendment';
import { Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType } from './types';

const order: Order = {
  id: '1',
  symbol: 'BTCUSDT',
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 100,
  amount: 1,
  remaining: 1,
  filled: 0,
  reduceOnly: false,
  lastUpdate: 0,
  timeInForce: OrderTimeInForce.GoodTillCancel,
};
const error = (changes: Partial<Order>, fields: { price?: number; amount?: number } = { price: 101 }) =>
  getOrderAmendmentError(exchangeMetadata.bingx, { ...order, ...changes }, fields);

describe('declared order amendment capability', () => {
  it.each([OrderTimeInForce.GoodTillCancel, OrderTimeInForce.PostOnly])('allows unfilled %s limits', (timeInForce) => {
    expect(error({ timeInForce })).toBeUndefined();
    expect(error({ timeInForce }, { amount: 2 })).toBeUndefined();
  });
  it.each([OrderTimeInForce.ImmediateOrCancel, OrderTimeInForce.FillOrKill, undefined])(
    'refuses unsupported or unknown TIF %s',
    (timeInForce) => {
      expect(error({ timeInForce })).toContain('GTC or PostOnly');
    },
  );
  it.each([OrderType.StopLoss, OrderType.TakeProfit])('preserves %s price edits but refuses quantity edits', (type) => {
    expect(error({ type, timeInForce: undefined })).toBeUndefined();
    expect(error({ type }, { amount: 2 })).toContain('only price changes');
  });
  it.each([OrderType.Market, OrderType.Conditional, OrderType.TrailingStopLoss])('refuses unsupported %s', (type) => {
    expect(error({ type })).toContain('GTC or PostOnly');
  });
  it('refuses filled, closed and close-position limits', () => {
    expect(error({ filled: 0.1 })).toContain('unfilled');
    expect(error({ status: OrderStatus.Closed })).toContain('open');
    expect(error({ close: true })).toContain('GTC or PostOnly');
  });
  it('keeps attached protection on a size edit and refuses a price replacement', () => {
    for (const protective of [{ stopLoss: 90 }, { takeProfit: 110 }]) {
      expect(error(protective)).toContain('without dropping its TP/SL');
      expect(error(protective, { amount: 2 })).toBeUndefined();
    }
  });
  it('refuses simultaneous edits before dispatch', () => {
    expect(error({}, { price: 101, amount: 2 })).toContain('not both');
  });
  it('uses the capability rather than an exchange-name condition', () => {
    expect(
      getOrderAmendmentError(
        { ...exchangeMetadata.bybit, orderAmendment: exchangeMetadata.bingx.orderAmendment },
        { ...order, timeInForce: OrderTimeInForce.ImmediateOrCancel },
        { price: 101 },
      ),
    ).toContain('GTC or PostOnly');
    expect(
      getOrderAmendmentError(
        exchangeMetadata.bybit,
        { ...order, timeInForce: OrderTimeInForce.ImmediateOrCancel },
        { price: 101 },
      ),
    ).toBeUndefined();
  });
});
