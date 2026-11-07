import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../../types';

/**
 * Binance `/fapi/v1/allOrders` row -> `HistoricalOrder`.
 *
 * Shape verified live (sheet row 2): `orderId, symbol, status, clientOrderId, price,
 * avgPrice, origQty, executedQty, type, origType, side, positionSide, reduceOnly,
 * stopPrice, time, updateTime, ...`.
 *
 * Like every venue's order history, `price` and `avgPrice` arrive as `"0"` rather than
 * absent when they do not apply, so both are dropped to `undefined` — a market order that
 * claims a price of zero corrupts any slippage or average calculation downstream.
 *
 * **`origType` beats `type`.** Binance rewrites `type` to `MARKET` once a stop triggers,
 * so a triggered STOP_MARKET is indistinguishable from a plain market order by `type`
 * alone; `origType` preserves what was actually placed.
 */

const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP: OrderType.StopLoss,
  STOP_MARKET: OrderType.StopLoss,
  TAKE_PROFIT: OrderType.TakeProfit,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TRAILING_STOP_MARKET: OrderType.TrailingStopLoss,
};

const ORDER_STATUS: Record<string, OrderStatus> = {
  NEW: OrderStatus.Open,
  PARTIALLY_FILLED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Canceled,
  EXPIRED_IN_MATCH: OrderStatus.Canceled,
  REJECTED: OrderStatus.Canceled,
};

const POSITION_SIDE: Record<string, PositionSide | 'both'> = {
  BOTH: 'both',
  LONG: PositionSide.Long,
  SHORT: PositionSide.Short,
};

const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBinanceHistoricalOrder = (row: Record<string, any>): HistoricalOrder | null => {
  const id = String(row.orderId ?? '');
  const symbol = String(row.symbol ?? '');
  const createdTime = Number(row.time ?? 0);
  if (!id || !symbol || !createdTime) {
    return null;
  }

  const side = String(row.side ?? '').toUpperCase();
  if (side !== 'BUY' && side !== 'SELL') {
    return null;
  }

  return {
    id,
    symbol,
    side: side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
    type: ORDER_TYPE[String(row.origType ?? '')] ?? ORDER_TYPE[String(row.type ?? '')] ?? OrderType.Limit,
    status: ORDER_STATUS[String(row.status ?? '')] ?? OrderStatus.Closed,
    price: toOptionalNumber(row.price),
    averagePrice: toOptionalNumber(row.avgPrice),
    amount: toNumber(row.origQty),
    filled: toNumber(row.executedQty),
    reduceOnly: Boolean(row.reduceOnly),
    positionSide: POSITION_SIDE[String(row.positionSide ?? '')],
    createdTime,
    updatedTime: Number(row.updateTime ?? 0) || undefined,
  };
};
