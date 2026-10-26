import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../../types';

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

/**
 * Present at all, as distinct from zero or unparseable.
 *
 * `realized_pnl` and `total_fee` are absent from the response unless the request asks for
 * them, and a genuine 0 is a real value — a breakeven close. Collapsing the two is the
 * exact defect that made woo look unsupportable.
 */
const hasValue = (value: unknown) =>
  value !== undefined && value !== null && value !== '' && Number.isFinite(parseFloat(String(value)));

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

    // Only present when the request carried `realized_pnl=true`; the field is absent
    // otherwise, which is why this uses a PRESENCE test rather than `toOptionalNumber`.
    // A flat closing order legitimately realizes exactly 0, and folding that to `undefined`
    // would make "closed at breakeven" indistinguishable from "the venue told us nothing" —
    // the same collapse that had woo written off as unsupportable.
    realizedPnl: hasValue(row.realized_pnl) ? toNumber(row.realized_pnl) : undefined,
    // Measured positive when charged on row 28 (`total_fee: 0.00181194` against a
    // `TRANSACTION_FEE` ledger row of `-0.00181194`), so it already matches the canonical
    // "positive = cost" convention and must NOT be negated.
    fees: hasValue(row.total_fee) ? toNumber(row.total_fee) : undefined,
    feeAsset: row.fee_asset ? String(row.fee_asset) : undefined,
    rebate: hasValue(row.total_rebate) ? toNumber(row.total_rebate) : undefined,
    rebateAsset: row.rebate_asset ? String(row.rebate_asset) : undefined,
  };
};
