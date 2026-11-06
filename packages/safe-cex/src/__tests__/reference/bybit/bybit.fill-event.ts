import { v } from '../../../utils/get-key';

/**
 * What actually EXECUTED on a bybit order frame.
 *
 * The `fill` event used to be emitted from the order's own `price` and `qty`,
 * and both are the wrong field:
 *
 *   * `price` is the order's LIMIT. On a market order bybit populates it with a
 *     slippage-protected bound, so a fill was reported roughly 10% adverse in
 *     both directions -- a buy at $0.057 against a $0.0519 market, sells at
 *     $0.047 against $0.0518. Nothing traded at those prices; it is the
 *     protection band. Reported from Discord 2026-09-29.
 *   * `qty` is the ORDER size, so a `PartiallyFilled` frame announced the whole
 *     order as filled.
 *
 * Every other bybit path already reads the execution fields -- `mapOrder`'s
 * `filled: cumExecQty`, the fills mapper's `avgPrice ?? avgFilledPrice ?? price`,
 * and `bybit.order-history.ts` -- so this is the one place that disagreed.
 *
 * **`avgPrice` is `""` before anything fills**, which is why parsing is
 * fallible-first rather than `parseFloat` with a `|| 0`: `parseFloat('')` is
 * `NaN`, and `NaN` passed to a UI reads as a fill at no price at all.
 */
const finiteNumber = (value: unknown): number | undefined => {
  if (value === undefined || value === null || value === '') return undefined;
  const parsed = typeof value === 'number' ? value : parseFloat(String(value));
  return Number.isFinite(parsed) ? parsed : undefined;
};

export interface BybitExecutionAmounts {
  readonly price: number;
  readonly amount: number;
}

export const resolveBybitExecution = (order: Record<string, any>): BybitExecutionAmounts => ({
  price: finiteNumber(v(order, 'avgPrice')) ?? finiteNumber(v(order, 'price')) ?? 0,
  amount: finiteNumber(v(order, 'cumExecQty')) ?? finiteNumber(v(order, 'qty')) ?? 0,
});
