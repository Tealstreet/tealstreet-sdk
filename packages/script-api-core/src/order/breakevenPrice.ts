import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { add, subtract } from '@tealstreet/safe-cex/src/utils/safe-math';

/**
 * The price at which closing what is LEFT of a position leaves it flat
 * overall, given what it has already banked and what it has paid in fees.
 *
 * It lives in the SDK because the answer is not web-specific and there were
 * FIVE of it in the repo a week ago — the chart's B/E line, the "breakeven all"
 * action, a hotkey template, the module API's `setBreakeven`, and the CLI's
 * `be` — giving four different prices for the same position. The map is in
 * `packages/safe-cex/CLAUDE.md` -> "Risk, PnL-at-price, ROE and break-even".
 *
 * **The direction depends on the SIDE.** Getting this wrong is not subtle and
 * it shipped: `getBreakevenPrice` had no side branch and was therefore the
 * SHORT answer applied to longs as well.
 *
 *   Long 10 @ 100 with $50 banked. Closing at P gives `(P - 100) * 10`, so
 *   `50 + (P - 100) * 10 = 0` at **P = 95** — banked profit lets a long exit
 *   LOWER and still be flat overall.
 *
 *   Short 10 @ 100 with $50 banked. Closing at P gives `(100 - P) * 10`, so
 *   the same $50 puts break-even at **105**.
 *
 * Fees are a cost, so they move it the opposite way from banked profit: a long
 * that has only paid commission breaks even ABOVE its entry.
 *
 * **No `isInverse` branch, deliberately.** Inverse belongs in SIZING, never in
 * price/PnL — `Position.contracts` is normalised to whole units of base, which
 * is what makes `size x price-distance` hold on an inverse contract as well as
 * a linear one. `safe-cex/utils/risk.ts` and `calculateReturns` contain no
 * `isInverse` for the same reason. A price-layer inverse branch is a bug.
 *
 * **Size is taken as a MAGNITUDE and the sign comes from `side`.** Callers
 * disagree about whether size is signed: `processTrades` keeps a signed
 * accumulator internally but emits `Math.max(totalBuy, totalSell)`, while a
 * live `Position` is unsigned with an explicit `side`. Deriving direction from
 * the sign of the size is exactly how the web copy went wrong.
 *
 * Rounding is deliberately NOT here, matching safe-cex's `nativePositionSize`:
 * callers round to their own market's tick when they need to.
 */
export interface BreakevenInputs {
  readonly entryPrice: number;
  /** Magnitude is used; a signed value from either convention is safe. */
  readonly size: number;
  readonly side: PositionSide;
  /** Profit already realised ON THIS POSITION. Not session PnL. */
  readonly bankedPnl?: number;
  /** Commission paid on this position, as a positive number. */
  readonly fees?: number;
}

export const breakevenPrice = ({ entryPrice, size, side, bankedPnl, fees }: BreakevenInputs): number => {
  // Guarded on the RESULT rather than on the inputs, because a finite net can
  // still divide into a non-finite per-contract figure — a denormal size and a
  // net of 1 gives Infinity, and the subtraction then returns -Infinity. The
  // whole point of this function is that its output reaches a stop order, and
  // -Infinity there is worse than a NaN because it looks like a price.
  if (!Number.isFinite(entryPrice)) return entryPrice;

  const magnitude = Math.abs(size || 0);
  // Nothing left to close: break-even IS the entry.
  if (!magnitude) return entryPrice;

  // Nothing banked and nothing paid: break-even is EXACTLY the entry, and
  // saying so beats computing it. `subtract(x, 0)` is not the identity —
  // safe-math rounds through `10 ** afterDecimal(x)`. Measured over 200k
  // values: no difference at all up to 15 significant digits, so every price
  // `parseFloat`-ed from a venue string is safe; it only bites at 16-17
  // digits, and `subtract(5e-324, 0)` is NaN outright because the factor
  // overflows. The fallback below catches that, but not computing it at all is
  // better on what is far and away the commonest input: a position that has
  // taken no partial profit.
  if (!bankedPnl && !fees) return entryPrice;

  const perContract = subtract(bankedPnl || 0, fees || 0) / magnitude;
  const result = side === PositionSide.Long ? subtract(entryPrice, perContract) : add(entryPrice, perContract);

  return Number.isFinite(result) ? result : entryPrice;
};
