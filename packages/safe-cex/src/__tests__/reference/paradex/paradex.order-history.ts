import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType } from '../../../types';

/**
 * Paradex `/orders-history` row -> `HistoricalOrder`.
 *
 * Shape verified live (sheet row 81): `id, account, market, side, type, size,
 * remaining_size, price, status, created_at, last_updated_at, cancel_reason, client_id,
 * avg_fill_price, flags, instruction, trigger_price, stp, seq_no`.
 *
 * **`status` is useless on this endpoint — every row reads `CLOSED`.** 100/100 rows on a
 * live account came back `CLOSED` regardless of whether the order filled or was rejected;
 * `/orders-history` is by definition the terminal set, and open orders live on `/orders`.
 * So the fill/cancel distinction has to come from `cancel_reason` and the size fields, and
 * a mapper that trusts `status` marks every cancellation as a completed order.
 *
 * **There is no `filled` field.** It is `size - remaining_size`, the same derivation the
 * live order path uses. An unfilled order carries `remaining_size === size` and
 * `avg_fill_price: ""`.
 *
 * Paradex is one-way only, so there is no position side to report.
 */

const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_LIMIT: OrderType.StopLoss,
  STOP_MARKET: OrderType.StopLoss,
  TAKE_PROFIT_LIMIT: OrderType.TakeProfit,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  STOP_LOSS_LIMIT: OrderType.StopLoss,
  STOP_LOSS_MARKET: OrderType.StopLoss,
};

/** Paradex sends `""` for an unset numeric field, and `"0"` for an unset price. */
const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapParadexHistoricalOrder = (row: Record<string, any>): HistoricalOrder | null => {
  const id = String(row.id ?? '');
  const symbol = String(row.market ?? '');
  const createdTime = toNumber(row.created_at);
  if (!id || !symbol || !createdTime) {
    return null;
  }

  const side = String(row.side ?? '').toUpperCase();
  if (side !== 'BUY' && side !== 'SELL') {
    return null;
  }

  const amount = toNumber(row.size);
  const remaining = toNumber(row.remaining_size);
  const filled = Math.max(0, amount - remaining);

  const rawStatus = String(row.status ?? '').toUpperCase();
  const cancelReason = String(row.cancel_reason ?? '');
  const status =
    rawStatus === 'NEW' || rawStatus === 'OPEN' || rawStatus === 'UNTRIGGERED'
      ? OrderStatus.Open
      : cancelReason
        ? OrderStatus.Canceled
        : OrderStatus.Closed;

  const flags: string[] = Array.isArray(row.flags) ? row.flags.map((f: any) => String(f).toUpperCase()) : [];
  const reduceOnly = flags.includes('REDUCE_ONLY');

  // Reduce-only is what separates a protective stop from an OPENING trigger, and paradex
  // places opening conditionals as non-reduce-only STOP_LIMIT. Classifying by type alone
  // reports every conditional Tealstreet placed here as a stop-loss. Mirrors
  // `reverseMapOrderType` on the live-order path so the two cannot disagree.
  const rawType = String(row.type ?? '').toUpperCase();
  const type =
    !reduceOnly && (rawType === 'STOP_LIMIT' || rawType === 'STOP_MARKET')
      ? OrderType.Conditional
      : (ORDER_TYPE[rawType] ?? OrderType.Limit);

  return {
    id,
    symbol,
    side: side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
    type,
    status,
    price: toOptionalNumber(row.price),
    averagePrice: toOptionalNumber(row.avg_fill_price),
    amount,
    filled,
    reduceOnly,
    createdTime,
    updatedTime: toNumber(row.last_updated_at) || undefined,
  };
};
