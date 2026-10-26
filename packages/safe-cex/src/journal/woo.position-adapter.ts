import type { Execution, HistoricalOrder } from '../types';
import type { PositionRecord } from './position.types';

import { OrderSide } from '../types';
import { mapFoldedInterval } from './fold.position-adapter';
import { reconstructPositionIntervals } from './position-intervals';

export const wooOrdersToPositionRecords = (
  orders: HistoricalOrder[],
  opts: { account: string; from: number; resolveSettleCurrency: (symbol: string) => string },
): PositionRecord[] => {
  const records: PositionRecord[] = [];
  for (const interval of reconstructPositionIntervals(wooOrdersToExecutions(orders))) {
    if (interval.closedAt === undefined || interval.closedAt < opts.from) continue;
    const record = mapFoldedInterval(interval, opts);
    if (record) records.push({ ...record, sourceAxis: 'orders' });
  }
  return records;
};

/**
 * WOO round trips are folded from the ORDER axis, not from fills.
 *
 * This is the only venue in the epic whose usable per-position PnL lives on orders. The
 * distinction is not cosmetic and drove the `'orders'` member added to `PositionSourceAxis`:
 *
 * - `/v3/trade/transactionHistory` (the fills axis) reports `realizedPnl: null` on **every**
 *   row, so a fold of fills there would refuse every interval;
 * - `/v1/client/trades` (the legacy fills axis) is the same — fee only, no PnL;
 * - `/v1/orders?realized_pnl=true` carries per-ORDER `realized_pnl`, `total_fee` **and**
 *   `position_side`, which is the only place all three appear together.
 *
 * **Why the ledger cannot grade this fold, measured rather than assumed.** WOO's
 * `/v1/client/transaction_history` looks like a ledger and is not one for this purpose: over a
 * 200-row window it returned 192 per-symbol `TRANSACTION_FEE` rows and only a handful of
 * `REALIZED_PNL` rows that carry **no symbol** and sit exactly 86 400 000 ms apart — daily
 * ACCOUNT-level aggregates. Phemex's `tradeAccountDetail` has the identical shape. So the
 * canonical gross-vs-gross identity has no counterparty on this venue, and a woo verdict must
 * SKIP the identity rather than appear to pass one.
 *
 * The per-record invariants (side, magnitude, fee convention, window containment) do not need
 * a ledger and still apply.
 */

/** Presence, not truthiness — an order that realizes exactly 0 is a real breakeven close. */
const optionalNum = (value: number | undefined): number | undefined =>
  value === undefined || !Number.isFinite(value) ? undefined : value;

/**
 * Filled orders -> `Execution[]`, so the shared interval reconstructor can fold them.
 *
 * One order becomes ONE synthetic execution. That is the correct grain here: WOO reports
 * `realized_pnl` and `total_fee` per order, already aggregated across that order's fills, so
 * splitting it further would have nothing to split them by — and joining the real per-fill
 * rows back on would double-count the PnL, since the fills axis reports none at all.
 *
 * `Execution.timestamp` is SECONDS while `HistoricalOrder.updatedTime` is milliseconds. That
 * conversion is the one this venue has already got wrong twice in different directions (the
 * request wants ms, the response reports fractional seconds), so it happens once, here.
 */
export const wooOrdersToExecutions = (orders: HistoricalOrder[]): Execution[] => {
  const executions: Execution[] = [];

  for (const order of orders) {
    // An order that never filled is real history — it is 27.4% of this account's orders and
    // the reason the order axis exists — but it moved no size, so it is not part of a round
    // trip and must not enter the fold.
    if (!(order.filled > 0)) continue;

    // `averagePrice` is the executed price. `price` is the LIMIT price and is wrong here:
    // using it would price every fill at where it was asked rather than where it traded,
    // which the magnitude invariant would then read as a PnL scale error.
    const price = order.averagePrice;
    if (!price || !(price > 0)) continue;

    const at = order.updatedTime ?? order.createdTime;
    if (!at || !(at > 0)) continue;

    executions.push({
      id: order.id,
      // Milliseconds -> seconds, once.
      timestamp: Math.floor(at / 1000),
      symbol: order.symbol,
      orderId: order.id,
      price,
      qty: order.filled,
      side: order.side === OrderSide.Buy ? OrderSide.Buy : OrderSide.Sell,
      // Carried through so the fold can key hedge legs without an order-axis join — WOO is
      // the one venue where the position side rides on the same row as the PnL.
      positionSide: order.positionSide,
      // Measured POSITIVE when charged on this route (`total_fee: 0.00181194` against a
      // `TRANSACTION_FEE` ledger row of `-0.00181194`), so it already matches the canonical
      // positive-is-a-cost convention and must NOT be negated here.
      commission: optionalNum(order.fees),
      commissionAsset: order.feeAsset,
      realizedPnl: optionalNum(order.realizedPnl),
    } as Execution);
  }

  return executions;
};
