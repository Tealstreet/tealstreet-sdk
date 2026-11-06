import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../../types';
import { ORDER_SIDE, ORDER_STATUS, ORDER_TYPE } from '../../../exchanges/bybit/bybit.types';

/**
 * Bybit `/v5/order/history` row -> `HistoricalOrder`.
 *
 * **Why this axis exists at all:** an order that never filled leaves no trace in fills,
 * executions or closed positions. A live 50-row sample was 24 Filled, 16 Deactivated and
 * 10 Cancelled — so more than half of the account's order activity is invisible to every
 * other axis in this repo. That is the whole justification for the order-history axis.
 *
 * Three shape traps, verified against a live row:
 *
 * - **`price` is `"0"` on market orders**, not absent. Passing it through means a market
 *   order claims it was placed at zero, and any average-price or slippage calculation
 *   downstream silently uses it.
 * - **`avgPrice` is `""` when nothing filled**, so `parseFloat` yields NaN rather than
 *   throwing. Both are mapped to `undefined`, which is what the optional field means.
 * - **`positionIdx` carries hedge-mode side**: 0 one-way, 1 long, 2 short. It is the only
 *   place the side of a hedged order survives, since `side` is the order's direction.
 *
 * `stopOrderType` is checked before `orderType`, because a triggered stop reports
 * `orderType: "Market"` and would otherwise be indistinguishable from a plain market order.
 */

const toOptionalNumber = (value: unknown) => {
  const raw = String(value ?? '').trim();
  if (!raw) return undefined;
  const parsed = parseFloat(raw);
  if (!Number.isFinite(parsed) || parsed === 0) return undefined;
  return parsed;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

const POSITION_SIDE_BY_IDX: Record<string, PositionSide | 'both'> = {
  '0': 'both',
  '1': PositionSide.Long,
  '2': PositionSide.Short,
};

export const mapBybitHistoricalOrder = (row: Record<string, any>): HistoricalOrder | null => {
  const id = String(row.orderId ?? '');
  const symbol = String(row.symbol ?? '');
  const createdTime = parseInt(String(row.createdTime ?? '0'), 10);
  if (!id || !symbol || !createdTime) {
    return null;
  }

  const side = ORDER_SIDE[String(row.side ?? '')];
  if (!side) {
    return null;
  }

  const stopType = String(row.stopOrderType ?? '').trim();
  const type = (stopType && ORDER_TYPE[stopType]) || ORDER_TYPE[String(row.orderType ?? '')] || OrderType.Limit;

  return {
    id,
    symbol,
    side: side as OrderSide,
    type,
    // Unknown statuses map to Closed rather than being dropped: a historical order is by
    // definition no longer working, and dropping it would understate order activity.
    status: ORDER_STATUS[String(row.orderStatus ?? '')] ?? OrderStatus.Closed,
    price: toOptionalNumber(row.price),
    averagePrice: toOptionalNumber(row.avgPrice),
    amount: toNumber(row.qty),
    filled: toNumber(row.cumExecQty),
    reduceOnly: Boolean(row.reduceOnly),
    positionSide: POSITION_SIDE_BY_IDX[String(row.positionIdx ?? '')],
    createdTime,
    updatedTime: parseInt(String(row.updatedTime ?? '0'), 10) || undefined,
  };
};
