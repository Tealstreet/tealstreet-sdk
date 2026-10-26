import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType } from '../../../types';

/**
 * Bitunix `/api/v1/futures/trade/get_history_orders` row -> `HistoricalOrder`.
 *
 * Shape verified live (sheet row 56): `orderId, symbol, qty, tradeQty, price, avgPrice,
 * side, orderType, effect, reduceOnly, status, fee, realizedPNL, positionMode,
 * marginMode, leverage, ctime, mtime, plus the tpPrice/slPrice trigger fields`.
 *
 * **`price` can be the literal string `"MARKET"`.** Not `""`, not `"0"` — the word. So
 * `parseFloat(row.price)` is `NaN` on every market order, and a mapper that assumes a
 * numeric string publishes NaN into the axis. Guard on `Number.isFinite`.
 *
 * **Rows are nested under `data.orderList`,** not `data`. This matters more than it looks:
 * bitunix answers a path that does not exist with `code: 404`, but answers a real path
 * whose rows you extracted from the wrong key with `code: 0` and nothing — so a wrong key
 * reads as "this account has no orders" rather than as a bug. That cost a probe cycle here.
 *
 * Filled quantity is `tradeQty`, and there is no per-order long/short field in hedge mode
 * (only the account-level `positionMode`), so no position side is reported.
 */

const ORDER_TYPE: Record<string, OrderType> = {
  MARKET: OrderType.Market,
  LIMIT: OrderType.Limit,
};

const ORDER_STATUS: Record<string, OrderStatus> = {
  INIT: OrderStatus.Open,
  NEW: OrderStatus.Open,
  PART_FILLED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELED: OrderStatus.Canceled,
  CANCELLED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Canceled,
  REJECTED: OrderStatus.Canceled,
};

const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBitunixHistoricalOrder = (row: Record<string, any>): HistoricalOrder | null => {
  const id = String(row.orderId ?? '');
  const symbol = String(row.symbol ?? '');
  const createdTime = parseInt(String(row.ctime ?? '0'), 10);
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
    type: ORDER_TYPE[String(row.orderType ?? '').toUpperCase()] ?? OrderType.Limit,
    status: ORDER_STATUS[String(row.status ?? '').toUpperCase()] ?? OrderStatus.Closed,
    price: toOptionalNumber(row.price),
    averagePrice: toOptionalNumber(row.avgPrice),
    amount: toNumber(row.qty),
    filled: toNumber(row.tradeQty),
    reduceOnly: row.reduceOnly === true || String(row.reduceOnly ?? '').toLowerCase() === 'true',
    createdTime,
    updatedTime: parseInt(String(row.mtime ?? '0'), 10) || undefined,
  };
};
