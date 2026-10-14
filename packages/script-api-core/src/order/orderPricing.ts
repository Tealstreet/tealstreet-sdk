import type { ExchangeName, Market, OrderType } from '@tealstreet/safe-cex/src/types';

import { roundToNearestMultiple, roundToNearestMultipleDown } from '@tealstreet/math/src/roundToNearestMultiple';
import { roundCloseSize, roundMarketOrderSize as roundMarketOrderSizeToStep } from './rounding';
import { exchangeMetadata, getExchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { OrderType as OrderTypes } from '@tealstreet/safe-cex/src/types';
import { DEFAULT_AMOUNT_PRECISION, DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { ScriptValidationError } from './ScriptValidationError';
import { shouldRoundSizeDown } from './shouldRoundSizeDown';

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

const priceTick = (price: number, market: Market): number => {
  let tick = market.precision.price || DEFAULT_PRICE_PRECISION;
  for (const band of market.precision.priceBands ?? []) {
    if (Math.abs(price) >= band.from) tick = band.tick;
  }
  return tick;
};

export const roundOrderPrice = (rawPrice: number, market: Market): number =>
  roundToNearestMultiple(rawPrice, priceTick(rawPrice, market));

/** Refuses a price under the venue's published minimum. Markets with no minimum pass. */
export const assertMinimumPrice = (price: number, market: Market): void => {
  const min = market.limits?.price?.min;
  if (min !== undefined && price < min) {
    throw new ScriptValidationError(`price ${price} is below the minimum ${min} for '${market.symbol}'`);
  }
};

const amountStep = (market: Market): number => market.precision.amount || DEFAULT_AMOUNT_PRECISION;

export const roundOrderSize = (rawSize: number, market: Market, exchangeName?: string): number =>
  shouldRoundSizeDown(market, exchangeName)
    ? roundToNearestMultipleDown(rawSize, amountStep(market))
    : roundToNearestMultiple(rawSize, amountStep(market));

/**
 * Plain round-to-nearest sizing with **no** `shouldRoundSizeDown` branch.
 * This is the rule `updateOrder` uses — an amend must not silently truncate
 * a resting order's size on the venues where placement rounds down.
 */
export const roundOrderSizeToNearest = (rawSize: number, market: Market): number =>
  roundToNearestMultiple(rawSize, amountStep(market));

/**
 * Sizing rule for reduce-only market closes: round to nearest, and fall back
 * to the raw size when rounding collapses to zero so a residual position
 * smaller than one lot step can still be closed.
 */
export const roundMarketCloseSize = (rawSize: number, market: Market): number =>
  roundCloseSize(rawSize, amountStep(market));

/**
 * Sizing for a market order — `createMarketOrder` and the script adapter's
 * reduce-only branch. It never applies `shouldRoundSizeDown`, which is the
 * documented market-vs-limit gap above.
 */
export const roundMarketOrderSize = (
  rawSize: number,
  market: Market,
  opts: { readonly reduceOnly?: boolean } = {},
): number => roundMarketOrderSizeToStep(rawSize, amountStep(market), { reduceOnly: opts.reduceOnly });

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
