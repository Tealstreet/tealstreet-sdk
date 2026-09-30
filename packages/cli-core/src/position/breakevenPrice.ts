import type { Position } from '@tealstreet/safe-cex/src/types';
import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { add, divide, subtract } from '@tealstreet/safe-cex/src/utils/safe-math';

/**
 * The price at which closing the REMAINING position leaves total PnL at zero.
 *
 * Derived from first principles rather than reused from the web app's
 * `getBreakevenPrice`, which operates on a `HistoricalPosition` whose
 * `contracts` is SIGNED and whose breakeven reads `unrealizedPnl` — semantics
 * that belong to the trade-history pipeline and do not transfer to a live
 * safe-cex `Position`.
 *
 * Long, 10 @ 100, $50 already banked: closing at P gives (P - 100) * 10, so
 * total is 50 + (P - 100) * 10 = 0 at P = 95. Banked profit lets you exit
 * LOWER and still be flat.
 *
 * Short, 10 @ 100, $50 banked: closing at P gives (100 - P) * 10, so total is
 * 50 + (100 - P) * 10 = 0 at P = 105 — the same profit moves a short's
 * breakeven UP. The direction flips with the side, which is why this takes the
 * side explicitly instead of relying on a sign convention.
 *
 * Fees are NOT included: a live `Position` does not carry them. So this is
 * break-even on price, and a round trip still costs the taker fee. The chart's
 * B/E line uses accumulated fee data the CLI has no access to, and the two can
 * differ for that reason.
 */
export const breakevenPrice = (
  position: Pick<Position, 'entryPrice' | 'side' | 'contracts' | 'realizedPnl'>,
  options: { readonly isInverse?: boolean } = {},
): number => {
  const { entryPrice, side, contracts, realizedPnl } = position;
  const size = Math.abs(contracts);
  const isLong = side === PositionSide.Long;

  // Nothing banked, or nothing left to close: breakeven IS the entry.
  if (!size || !realizedPnl || !Number.isFinite(realizedPnl)) return entryPrice;

  const perContract = realizedPnl / size;

  /**
   * **An inverse contract settles in the BASE currency and its PnL is
   * reciprocal in price**, so the linear formula below is not merely
   * imprecise there — it adds a base-denominated quantity to a quote-
   * denominated price. `useOpenPositionsColumns` branches the same way for its
   * worst-case-loss column (`contracts * (1/stop - 1/entry)`).
   *
   * Long: realized + contracts * (1/entry - 1/P) = 0
   *   =>  1/P = 1/entry + realized/contracts
   * and the sign flips for a short. Banked profit still moves a long's
   * breakeven DOWN, the same direction as linear — worth checking, because
   * working in reciprocal space makes that easy to get backwards.
   */
  if (options.isInverse) {
    const inverseEntry = divide(1, entryPrice);
    const target = isLong ? add(inverseEntry, perContract) : subtract(inverseEntry, perContract);
    // A banked loss large enough to flip the reciprocal past zero has no
    // breakeven price at all; say so by staying at entry rather than emitting
    // a negative or infinite price into an order.
    if (!Number.isFinite(target) || target <= 0) return entryPrice;
    return divide(1, target);
  }

  return isLong ? subtract(entryPrice, perContract) : add(entryPrice, perContract);
};
