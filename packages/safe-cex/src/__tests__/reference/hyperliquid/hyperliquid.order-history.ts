import type { HistoricalOrder } from '../../../types';

import { OrderSide, OrderStatus, OrderType } from '../../../types';

/**
 * Hyperliquid `POST /info {type: "historicalOrders"}` row -> `HistoricalOrder`.
 *
 * Row shape verified live (sheet row 50): `{ order: { coin, side, limitPx, sz, oid,
 * timestamp, triggerCondition, isTrigger, triggerPx, children, isPositionTpsl, reduceOnly,
 * orderType, origSz, tif, cloid }, status, statusTimestamp }`.
 *
 * Three things about this endpoint, all measured rather than assumed:
 *
 * **It emits one row per status TRANSITION, not per order.** A live account returned
 * **2000 rows carrying only 1020 distinct `oid`s** — the 980 surplus rows are the `open`
 * state of orders that later filled or cancelled, and the sampled `open` and `canceled`
 * rows were byte-identical apart from `status`/`statusTimestamp`. Treating each row as an
 * order roughly doubles the count and reports live orders that are long gone. Collapse
 * with `collapseHyperliquidOrderHistory` first. Note this is the *inverse* of the okx and
 * bybit traps, where an id dedupe destroyed real rows — which is exactly why it has to be
 * measured per venue and never generalised.
 *
 * **The time window is IGNORED.** Passing `startTime`/`endTime` returned a byte-identical
 * 2000 rows. It is a fixed rolling snapshot, so the window must be applied client-side and
 * `paginateHyperliquidByTime` would degenerate to a single call. The 2000-row ceiling is a
 * real retention limit on this axis, not a page size to walk past.
 *
 * **`sz` is REMAINING, `origSz` is the original.** A `filled` row carries `sz: "0.0"` with
 * `origSz: "170.0"`; a `canceled` row carries `sz === origSz`. So `filled = origSz - sz`,
 * and reading `sz` as the amount reports every completed order as size zero.
 */

/** `A` is ask (sell) and `B` is bid (buy) — the venue's own abbreviations. */
const SIDE: Record<string, OrderSide> = {
  A: OrderSide.Sell,
  B: OrderSide.Buy,
};

const ORDER_TYPE: Record<string, OrderType> = {
  limit: OrderType.Limit,
  market: OrderType.Market,
  'stop market': OrderType.StopLoss,
  'stop limit': OrderType.StopLoss,
  'take profit market': OrderType.TakeProfit,
  'take profit limit': OrderType.TakeProfit,
};

/**
 * Hyperliquid publishes a long and growing list of terminal statuses — this account alone
 * produced `canceled`, `reduceOnlyCanceled`, `badAloPxRejected`, `minTradeNtlRejected` and
 * `reduceOnlyRejected`. Enumerating them would silently mis-handle the next one added, so
 * only the two live states are named and everything else is terminal-not-filled.
 */
const mapStatus = (status: string): OrderStatus => {
  if (status === 'open' || status === 'triggered') return OrderStatus.Open;
  if (status === 'filled') return OrderStatus.Closed;
  return OrderStatus.Canceled;
};

const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/**
 * `statusTimestamp` alone cannot order the two rows of an order that filled immediately.
 *
 * A resting order's terminal row carries a LATER `statusTimestamp` than its `open` row, but
 * an order that fills on placement carries the SAME value on both — the live sample had
 * `timestamp === statusTimestamp === 1786522506093` on the filled row. Breaking that tie by
 * array position let `open` win, and the whole axis came back with **zero filled orders**
 * out of 490 while the raw snapshot held 315. The conformance check still passed, because
 * ids and row counts were all correct; only the status census showed it.
 *
 * So a terminal state always outranks `open` at an equal timestamp, which is also
 * independent of the order the venue happens to return rows in.
 */
/**
 * Ranked rather than a terminal/non-terminal boolean, so that EVERY equal-timestamp pair
 * resolves the same way regardless of the order the venue returned the rows in. A boolean
 * still fell through to array position when both rows were terminal.
 */
const statusRank = (status: string) => {
  if (status === 'open' || status === 'triggered') return 0;
  if (status === 'filled') return 2;
  return 1;
};

const isLaterState = (candidate: Record<string, any>, incumbent: Record<string, any>) => {
  const candidateTime = toNumber(candidate.statusTimestamp);
  const incumbentTime = toNumber(incumbent.statusTimestamp);
  if (candidateTime !== incumbentTime) return candidateTime > incumbentTime;
  return statusRank(String(candidate.status ?? '')) > statusRank(String(incumbent.status ?? ''));
};

/**
 * Keep only the latest row per `oid`, by `statusTimestamp`.
 *
 * Exported separately from the mapper so the collapse is testable on its own — the
 * measured 2000-rows-to-1020-orders behaviour is the part most likely to regress, and it
 * is invisible from a single mapped row.
 */
export const collapseHyperliquidOrderHistory = (rows: Array<Record<string, any>>): Array<Record<string, any>> => {
  const latest = new Map<string, Record<string, any>>();

  for (const row of rows) {
    const oid = String(row?.order?.oid ?? '');
    if (!oid) continue;

    const existing = latest.get(oid);
    if (!existing || isLaterState(row, existing)) {
      latest.set(oid, row);
    }
  }

  return [...latest.values()];
};

export const mapHyperliquidHistoricalOrder = (
  row: Record<string, any>,
  resolveSymbol: (coin: string) => string | undefined,
): HistoricalOrder | null => {
  const order = row?.order;
  const id = String(order?.oid ?? '');
  const coin = String(order?.coin ?? '');
  const createdTime = toNumber(order?.timestamp);
  if (!id || !coin || !createdTime) {
    return null;
  }

  const side = SIDE[String(order?.side ?? '')];
  if (!side) {
    return null;
  }

  const amount = toNumber(order?.origSz);
  const remaining = toNumber(order?.sz);
  const filled = Math.max(0, amount - remaining);

  return {
    id,
    // Falling back to the raw coin rather than dropping the row: an unresolvable market —
    // a delisted asset, or a HIP-3 dex not in this store — must not erase order history.
    symbol: resolveSymbol(coin) || coin,
    side,
    type: ORDER_TYPE[String(order?.orderType ?? '').toLowerCase()] ?? OrderType.Limit,
    status: mapStatus(String(row?.status ?? '')),
    // For a trigger order `limitPx` is the execution price and `triggerPx` is what fires
    // it; reporting the trigger as the price would misstate where the order rested.
    price: toOptionalNumber(order?.limitPx),
    // No average fill price exists on this endpoint — `historicalOrders` carries no `avgPx`
    // equivalent. Left absent rather than derived from `limitPx`, which would be a guess.
    amount,
    filled,
    reduceOnly: order?.reduceOnly === true,
    createdTime,
    updatedTime: toNumber(row?.statusTimestamp) || undefined,
  };
};
