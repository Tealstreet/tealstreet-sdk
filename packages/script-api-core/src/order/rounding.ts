import { roundToNearestMultiple } from '@tealstreet/math/src/roundToNearestMultiple';

/**
 * The order-sizing arithmetic, in the one place it is defined.
 *
 * This used to be a preview-only copy that "mirrored" `roundMarketCloseSize`
 * in `apps/web`, which mirrored an inline copy in `createMarketOrder`. Three
 * statements of one rule, and the drift between them was real: the web close
 * rule fell back unconditionally while `createMarketOrder` gated the fallback
 * on `reduceOnly`.
 *
 * Now `script-api-core/src/order/orderPricing.ts` DELEGATES here. It stays a
 * separate function there because its callers hold a safe-cex `Market` and
 * this package deliberately does not import safe-cex — so the split is a
 * SHAPE adapter over one rule, not a second rule.
 *
 * One thing the host has that this cannot: `shouldRoundSizeDown` gates
 * round-down on specific venues and needs `apps/web`'s exchange metadata. It
 * stays in `orderPricing.ts`, on the limit-placement rule that uses
 * it — a market order has never applied it.
 */

export const roundToStep = (value: number, step: number): number =>
  step > 0 ? roundToNearestMultiple(value, step) : value;

/**
 * The market-order sizing rule, for both the preview and the host.
 *
 * `reduceOnly` decides whether the collapse-to-zero fallback applies, and that
 * is the divergence this function exists to end: `createMarketOrder` gated the
 * fallback on reduce-only while `roundMarketCloseSize` beside it fell back
 * unconditionally, so the same residual position sized differently depending
 * on which door it came through.
 *
 * The fallback matters because a residual position smaller than one lot step
 * rounds to zero, and a zero-size order is not a no-op — on venues that support
 * infinity stops it reads as the WHOLE position. Falling back to the raw size
 * keeps the residual closable instead of rejecting it client-side.
 *
 * Round-DOWN is not here. `shouldRoundSizeDown` is venue-specific, needs
 * `apps/web`'s exchange metadata, and applies to limit placement rather than to
 * market orders — `roundOrderSize` in `orderPricing.ts` owns it.
 */
export const roundMarketOrderSize = (
  value: number,
  step: number,
  opts: { readonly reduceOnly?: boolean } = {},
): number => {
  const rounded = roundToStep(value, step);
  return rounded > 0 || !opts.reduceOnly ? rounded : value;
};

/** A reduce-only close: `roundMarketOrderSize` with the fallback always on. */
export const roundCloseSize = (value: number, step: number): number =>
  roundMarketOrderSize(value, step, { reduceOnly: true });
