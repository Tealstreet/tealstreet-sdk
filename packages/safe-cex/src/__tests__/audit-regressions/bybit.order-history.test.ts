import { describe, expect, it } from 'vitest';

import { mapBybitHistoricalOrder } from '../reference/bybit/bybit.order-history';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';

/** Shape captured live from `/v5/order/history` (sheet row 6). */
const order = (over: Record<string, any> = {}) => ({
  orderId: 'dfc8d0fc-c19a-46ad-b5ed-4d1ddd1d0f23',
  symbol: 'DOGEUSDT',
  side: 'Buy',
  orderType: 'Market',
  stopOrderType: '',
  orderStatus: 'Filled',
  price: '0.0717',
  avgPrice: '0.07169',
  qty: '138',
  cumExecQty: '138',
  reduceOnly: true,
  positionIdx: 2,
  createdTime: '1786522921789',
  updatedTime: '1786522927726',
  ...over,
});

describe('bybit historical orders', () => {
  it('maps a filled order', () => {
    const mapped = mapBybitHistoricalOrder(order())!;

    expect(mapped.id).toBe('dfc8d0fc-c19a-46ad-b5ed-4d1ddd1d0f23');
    expect(mapped.side).toBe(OrderSide.Buy);
    expect(mapped.status).toBe(OrderStatus.Closed);
    expect(mapped.filled).toBe(138);
  });

  it('drops `price: "0"` on market orders instead of claiming a zero price', () => {
    // Bybit sends "0", not absent. Passing it through makes a market order look placed at
    // zero, and every downstream slippage/average calculation silently uses it.
    expect(mapBybitHistoricalOrder(order({ price: '0' }))!.price).toBeUndefined();
  });

  it('drops an empty `avgPrice` instead of producing NaN', () => {
    // "" parses to NaN rather than throwing, so an unfilled order would carry NaN forever.
    expect(mapBybitHistoricalOrder(order({ avgPrice: '' }))!.averagePrice).toBeUndefined();
  });

  it('keeps orders that NEVER FILLED — the reason this axis exists', () => {
    // A live 50-row sample was 24 Filled, 16 Deactivated, 10 Cancelled. Those 26 are
    // invisible to fills, executions and closed positions alike.
    const deactivated = mapBybitHistoricalOrder(
      order({ orderStatus: 'Deactivated', cumExecQty: '0', avgPrice: '' }),
    )!;

    expect(deactivated).not.toBeNull();
    expect(deactivated.filled).toBe(0);
    expect(deactivated.status).toBe(OrderStatus.Closed);
  });

  it('reads hedge side from positionIdx, the only place it survives', () => {
    expect(mapBybitHistoricalOrder(order({ positionIdx: 1 }))!.positionSide).toBe(PositionSide.Long);
    expect(mapBybitHistoricalOrder(order({ positionIdx: 2 }))!.positionSide).toBe(PositionSide.Short);
    expect(mapBybitHistoricalOrder(order({ positionIdx: 0 }))!.positionSide).toBe('both');
  });

  it('prefers stopOrderType, since a triggered stop reports orderType Market', () => {
    const stop = mapBybitHistoricalOrder(order({ stopOrderType: 'StopLoss', orderType: 'Market' }))!;
    expect(stop.type).toBe(OrderType.StopLoss);

    const plain = mapBybitHistoricalOrder(order({ stopOrderType: '', orderType: 'Market' }))!;
    expect(plain.type).toBe(OrderType.Market);
  });

  it('drops rows with no id, symbol, timestamp or side', () => {
    expect(mapBybitHistoricalOrder(order({ orderId: '' }))).toBeNull();
    expect(mapBybitHistoricalOrder(order({ createdTime: '0' }))).toBeNull();
    expect(mapBybitHistoricalOrder(order({ side: 'Sideways' }))).toBeNull();
  });
});
