import type { HistoricalOrder } from '../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';

/**
 * WOO `GET /v1/orders` row -> `HistoricalOrder`.
 *
 * Shape verified live (sheet row 28): `order_id, symbol, side, type, status, price,
 * quantity, executed, average_executed_price, reduce_only, position_side, created_time,
 * updated_time, total_fee, fee_asset, realized_pnl, leverage, margin_mode, visible`.
 *
 * **`created_time` and `updated_time` are fractional SECONDS in a string**
 * (`"1786536593.515"`), not integer milliseconds like every other venue in this axis.
 * Passing one straight through puts the whole account in January 1970, where a window
 * filter then discards all of it — an empty axis rather than a visibly wrong date.
 *
 * Rows arrive newest-first and `page` genuinely advances (page 2 returned strictly older
 * orders), which is worth stating because WOO's *trade* route returns the OLDEST rows
 * first — see the WOO note in `docs/TRADE_HISTORY_VENUE_STATUS_v1.md`. The two routes do
 * not share an ordering convention.
 */

const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  IOC: OrderType.Limit,
  FOK: OrderType.Limit,
  POST_ONLY: OrderType.Limit,
  ASK: OrderType.Limit,
  BID: OrderType.Limit,
};

const ORDER_STATUS: Record<string, OrderStatus> = {
  NEW: OrderStatus.Open,
  INCOMPLETE: OrderStatus.Open,
  PARTIAL_FILLED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  COMPLETED: OrderStatus.Closed,
  CANCELLED: OrderStatus.Canceled,
  CANCELED: OrderStatus.Canceled,
  REJECTED: OrderStatus.Canceled,
};

/** Seconds -> milliseconds. The fractional part is real; truncate only after scaling. */
const toMilliseconds = (value: unknown) => {
  const seconds = parseFloat(String(value ?? ''));
  return Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds * 1000) : 0;
};

const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapWooHistoricalOrder = (
  row: Record<string, any>,
  resolveSymbol: (marketId: string) => string | undefined,
): HistoricalOrder | null => {
  const id = String(row.order_id ?? '');
  const marketId = String(row.symbol ?? '');
  const createdTime = toMilliseconds(row.created_time);
  if (!id || !marketId || !createdTime) {
    return null;
  }

  const side = String(row.side ?? '').toUpperCase();
  if (side !== 'BUY' && side !== 'SELL') {
    return null;
  }

  const positionSide = String(row.position_side ?? '').toUpperCase();

  return {
    id,
    symbol: resolveSymbol(marketId) || marketId,
    side: side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
    type: ORDER_TYPE[String(row.type ?? '').toUpperCase()] ?? OrderType.Limit,
    status: ORDER_STATUS[String(row.status ?? '').toUpperCase()] ?? OrderStatus.Closed,
    price: toOptionalNumber(row.price),
    averagePrice: toOptionalNumber(row.average_executed_price),
    amount: toNumber(row.quantity),
    filled: toNumber(row.executed),
    reduceOnly: row.reduce_only === true || String(row.reduce_only ?? '').toLowerCase() === 'true',
    positionSide: positionSide === 'LONG' ? PositionSide.Long : positionSide === 'SHORT' ? PositionSide.Short : 'both',
    createdTime,
    updatedTime: toMilliseconds(row.updated_time) || undefined,
  };
};
