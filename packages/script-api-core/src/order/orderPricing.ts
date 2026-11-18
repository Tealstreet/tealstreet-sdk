import type { ExchangeName, Market, OrderType, PlaceOrderOpts } from '@tealstreet/safe-cex/src/types';
import type { NativeAmountMarket } from './nativeAmountRules';

import { roundToNearestMultiple, roundToNearestMultipleDown } from '@tealstreet/math/src/roundToNearestMultiple';
import { roundCloseSize, roundMarketOrderSize as roundMarketOrderSizeToStep } from './rounding';
import { exchangeMetadata, getExchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { OrderType as OrderTypes } from '@tealstreet/safe-cex/src/types';
import { venueUsesBaseAmountRails } from '@tealstreet/safe-cex/src/utils/nativeAmountRules';
import { DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { assertNativeReduceAmount, nativeAmountRules } from './nativeAmountRules';
import { ScriptValidationError } from './ScriptValidationError';
import { shouldRoundSizeDown } from './shouldRoundSizeDown';

export { venueUsesBaseAmountRails } from '@tealstreet/safe-cex/src/utils/nativeAmountRules';

/**
 * Venue-correct price/size rounding — the one place `apps/web` states it.
 *
 * The ARITHMETIC lives in `./rounding`, which has no
 * safe-cex dependency and is therefore reachable from the preview resolver
 * too. These functions are the `Market`-shaped adapter over it: they turn
 * `market.precision.*` into a step and answer the venue questions
 * (`shouldRoundSizeDown`, `supportsInfinityStops`) the package cannot see.
 *
 * They exist because non-UI order paths — the script `CoreAdapter`, and
 * through it the AI order path — need the same numbers as the OEMS helpers
 * without inheriting their toasts, sound, fat-finger modal or optimistic order
 * ids.
 *
 * ## The five size rules, and why they are five
 *
 * | Path                          | Rule                                        |
 * | ----------------------------- | ------------------------------------------- |
 * | `placeOrder` limit / non-reduce | `roundOrderSize` — round-down branch        |
 * | market order                    | `roundMarketOrderSize` — reduce-only fallback |
 * | amend                           | `roundOrderSizeToNearest` — never truncate  |
 * | reduce-only close               | `roundMarketCloseSize` — always falls back  |
 *
 * **Market orders and limit orders disagree about round-down, deliberately.**
 * `createMarketOrder` has never applied `shouldRoundSizeDown`; `createLimitOrder`
 * has. The round-down exists so a Hyperliquid SPOT buy cannot exceed the
 * balance held, which applies to a market buy just as much — so this is a
 * latent gap, not a rule. It is preserved rather than fixed here because
 * changing it moves money; `roundDown` is an explicit argument so the
 * difference is visible at both call sites instead of hidden in two files.
 */

export const roundOrderPrice = (rawPrice: number, market: Market): number =>
  roundToNearestMultiple(rawPrice, market.precision.price || DEFAULT_PRICE_PRECISION);

/** Refuses a price under the venue's published minimum. Markets with no minimum pass. */
export const assertMinimumPrice = (price: number, market: Market): void => {
  const min = market.limits?.price?.min;
  if (min !== undefined && price < min) {
    throw new ScriptValidationError(`price ${price} is below the minimum ${min} for '${market.symbol}'`);
  }
};

export const roundOrderSize = (rawSize: number, market: Market, exchangeName?: string): number =>
  shouldRoundSizeDown(market, exchangeName)
    ? roundToNearestMultipleDown(rawSize, nativeAmountRules(market, exchangeName).step)
    : roundToNearestMultiple(rawSize, nativeAmountRules(market, exchangeName).step);

/**
 * Plain round-to-nearest sizing with **no** `shouldRoundSizeDown` branch.
 * This is the rule `updateOrder` uses — an amend must not silently truncate
 * a resting order's size on the venues where placement rounds down.
 */
export const roundOrderSizeToNearest = (rawSize: number, market: Market, exchangeName?: string): number =>
  roundToNearestMultiple(rawSize, nativeAmountRules(market, exchangeName).step);

/**
 * Sizing rule for reduce-only market closes: round to nearest, and fall back
 * to the raw size when rounding collapses to zero so a residual position
 * smaller than one lot step can still be closed.
 */
export const roundMarketCloseSize = (rawSize: number, market: Market, exchangeName?: string): number => {
  assertNativeReduceAmount(rawSize, market, exchangeName);
  return roundCloseSize(rawSize, nativeAmountRules(market, exchangeName).step);
};

/** Percentage intent floors partial closes; a full close retains its exact native size. */
export const roundPercentageCloseSize = (
  fullNative: number | null,
  percent: number,
  market: NativeAmountMarket,
  exchangeName?: string,
): number => {
  if (fullNative === null || !Number.isFinite(fullNative) || fullNative <= 0) {
    throw new ScriptValidationError(`Native position size is unknown or non-positive for '${market.symbol}'`);
  }
  if (!Number.isFinite(percent) || percent <= 0 || percent > 100) {
    throw new ScriptValidationError(`Invalid close percentage ${percent}; expected more than 0 and at most 100`);
  }
  if (percent === 100) return fullNative;
  const { step } = nativeAmountRules(market, exchangeName);
  const requested = fullNative * (percent / 100);
  const amount = roundToNearestMultipleDown(requested, step);
  if (!Number.isFinite(amount) || amount <= 0) {
    throw new ScriptValidationError(
      `Percentage close size ${requested} floors to 0 at native step ${step} for '${market.symbol}'`,
    );
  }
  return amount;
};

/**
 * Sizing for a market order — `createMarketOrder` and the script adapter's
 * reduce-only branch. It never applies `shouldRoundSizeDown`, which is the
 * documented market-vs-limit gap above.
 */
export const roundMarketOrderSize = (
  rawSize: number,
  market: Market,
  opts: { readonly reduceOnly?: boolean; readonly exchangeName?: string } = {},
): number => {
  if (opts.reduceOnly) assertNativeReduceAmount(rawSize, market, opts.exchangeName);
  return roundMarketOrderSizeToStep(rawSize, nativeAmountRules(market, opts.exchangeName).step, {
    reduceOnly: opts.reduceOnly,
  });
};

const STOP_LIKE_ORDER_TYPES: ReadonlySet<OrderType> = new Set([
  OrderTypes.StopLoss,
  OrderTypes.TakeProfit,
  OrderTypes.TrailingStopLoss,
]);

export const isStopLikeOrderType = (type: OrderType): boolean => STOP_LIKE_ORDER_TYPES.has(type);

/**
 * A stop-like order with `amount: 0` is a Full ("infinity") TP/SL covering the
 * whole position — legal only where the venue supports it. A bare
 * `amount <= 0` guard silently breaks it, which is why every size check on a
 * stop path has to ask this first.
 *
 * Two exports rather than one combined predicate, because the two callers know
 * different things: `createStopLossOrder` places nothing but stops and runs its
 * check before it has resolved a `type`, while the script adapter sees every
 * order type and must ask. Forcing one signature on both would mean moving a
 * guard past the error-precedence it currently has.
 *
 * Unknown exchange names THROW here, via `getExchangeMetadata`. The copy in
 * `createStopLossOrder` indexed `exchangeMetadata` directly and got a TypeError
 * on `undefined`; a named error is the better of the two, and neither is a
 * reachable state with a connected account.
 */
export const venueSupportsInfinityStops = (exchangeName: ExchangeName): boolean =>
  getExchangeMetadata(exchangeName).supportsInfinityStops;

/** Protective kinds close by nature, even when the caller omits reduceOnly. */
export const assertCloseLikeOrderAmount = (
  amount: number,
  order: Pick<PlaceOrderOpts, 'type' | 'reduceOnly' | 'close'>,
  market: NativeAmountMarket,
  exchangeName?: string,
): void => {
  if (!venueUsesBaseAmountRails(exchangeName)) return;
  const protective = isStopLikeOrderType(order.type);
  if (!protective && !order.reduceOnly && !order.close) return;
  if (amount === 0 && protective && venueSupportsInfinityStops(exchangeName)) return;
  assertNativeReduceAmount(amount, market, exchangeName);
};

/**
 * Whether the venue wants reduce-only orders shrunk against what is already
 * resting (hyperliquid, woo). The gate for `downSizeReduceOrder`.
 *
 * An unrecognised venue answers `false` rather than throwing, and that is the
 * OPPOSITE default from `venueSupportsInfinityStops` above — deliberately,
 * because failing closed points in opposite directions for the two questions.
 * "Does this venue need its reduce-only orders shrunk?" — if we do not know,
 * do not shrink the user's order; the venue rejects the excess, which is what
 * happened before the clamp existed. "Does this venue support a zero-size
 * infinity stop?" — if we do not know, refuse, because a zero-size order that
 * is not read as `Full` is an order for nothing.
 *
 * This matters beyond a bad name: `getExchangeMetadata` throws, and this is
 * called on EVERY reduce-only write, so a venue missing from the metadata map
 * failed the write rather than skipping an optimisation.
 */
export const venueAutoDownsizesReduceOrders = (exchangeName: ExchangeName): boolean =>
  Boolean(exchangeMetadata[exchangeName]?.autoDownSizeReduceOrders);

/** Resting close orders hold position size on this venue (Bitget family): only the free part closes. */
export const venueReduceOrdersLockPositionSize = (exchangeName: ExchangeName): boolean =>
  Boolean(exchangeMetadata[exchangeName]?.reduceOrdersLockPositionSize);
