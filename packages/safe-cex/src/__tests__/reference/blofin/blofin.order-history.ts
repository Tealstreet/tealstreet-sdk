import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../../types';

/**
 * BloFin `/api/v1/trade/orders-history` row -> `HistoricalOrder`.
 *
 * Shape verified live (sheet row 41): `orderId, clientOrderId, instId, marginMode,
 * positionSide, side, orderType, price, size, reduceOnly, leverage, state, filledSize,
 * pnl, averagePrice, fee, createTime, updateTime, orderCategory, tp/sl trigger fields`.
 *
 * `state` is lower-case here (`filled` / `canceled` / `live`), unlike bybit's capitalised
 * statuses — mapping it through a shared table would silently fall through to the default
 * and mark every order Closed.
 */

const ORDER_TYPE: Record<string, OrderType> = {
  limit: OrderType.Limit,
  market: OrderType.Market,
  post_only: OrderType.Limit,
  fok: OrderType.Limit,
  ioc: OrderType.Limit,
  trigger: OrderType.Conditional,
};

const ORDER_STATUS: Record<string, OrderStatus> = {
  live: OrderStatus.Open,
  partially_filled: OrderStatus.Open,
  filled: OrderStatus.Closed,
  canceled: OrderStatus.Canceled,
  cancelled: OrderStatus.Canceled,
};

const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBlofinHistoricalOrder = (
  row: Record<string, any>,
  resolveSymbol: (instId: string) => string | undefined,
): HistoricalOrder | null => {
  const id = String(row.orderId ?? '');
  const instId = String(row.instId ?? '');
  const createdTime = parseInt(String(row.createTime ?? '0'), 10);
  if (!id || !instId || !createdTime) {
    return null;
  }

  const side = String(row.side ?? '').toLowerCase();
  if (side !== 'buy' && side !== 'sell') {
    return null;
  }

  const positionSide = String(row.positionSide ?? '').toLowerCase();

  return {
    id,
    symbol: resolveSymbol(instId) || instId,
    side: side === 'buy' ? OrderSide.Buy : OrderSide.Sell,
    type: ORDER_TYPE[String(row.orderType ?? '').toLowerCase()] ?? OrderType.Limit,
    status: ORDER_STATUS[String(row.state ?? '').toLowerCase()] ?? OrderStatus.Closed,
    price: toOptionalNumber(row.price),
    averagePrice: toOptionalNumber(row.averagePrice),
    amount: toNumber(row.size),
    filled: toNumber(row.filledSize),
    reduceOnly: String(row.reduceOnly ?? '').toLowerCase() === 'true' || row.reduceOnly === true,
    positionSide:
      positionSide === 'net' ? 'both' : positionSide === 'short' ? PositionSide.Short : PositionSide.Long,
    createdTime,
    updatedTime: parseInt(String(row.updateTime ?? '0'), 10) || undefined,
  };
};
