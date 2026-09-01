import type { ExchangeName, Market, Order, PlaceOrderOpts, Position, Ticker } from '@tealstreet/safe-cex/src/types';

import { OrderType } from '@tealstreet/safe-cex/src/types';
import { downSizeReduceOrder } from './downSizeReduceOrder';
import {
  isStopLikeOrderType,
  roundMarketOrderSize,
  roundOrderPrice,
  roundOrderSize,
  venueAutoDownsizesReduceOrders,
  venueSupportsInfinityStops,
} from './orderPricing';
import { ScriptValidationError } from './ScriptValidationError';

/**
 * Every number that reaches a venue, in the one place it is decided.
 *
 * This used to live inside `createCoreAdapter`'s closure, reachable only by
 * holding an adapter — which meant holding `HostBindings`, an account name and
 * an account-to-exchange resolver. The CLI's trading commands have none of
 * those: they run from three different hosts (the REPL, the script adapter,
 * and `worker-host/router.ts`, which is the web app's in-browser CLI worker),
 * and two of them build a `CommandContext` with a bare exchange handle and no
 * account at all. So the commands could not reach the policy, and wrote
 * straight to `exchange.placeOrder` with no rounding, no minimum-size refusal,
 * no infinity-stop check and no reduce-only clamp.
 *
 * Lifting it out takes an ACCOUNT-shaped dependency off the policy and leaves
 * a MARKET-shaped one, which every caller already has. `createCoreAdapter`
 * calls this too — `prepareOrderIsTheOnlyPolicy.test.ts` fails the build if a
 * second copy of the arithmetic appears.
 *
 * What is deliberately NOT here: toasts, sound, optimistic ids, fat-finger.
 * Fat-finger blocks an order behind a modal, which is right for a human at a
 * button and wrong for a headless script or a CLI pipeline. The hosts keep
 * that half.
 */

export interface OrderPolicyContext {
  readonly market: Market;
  /** Lower-cased venue id. Absent means "unknown venue": round-down and the
   *  infinity-stop carve-out both fail CLOSED rather than being assumed. */
  readonly exchangeName?: ExchangeName;
}

export interface ReduceOnlyClampContext extends OrderPolicyContext {
  readonly positions: readonly Position[];
  readonly restingOrders: readonly Order[];
  readonly ticker?: Ticker;
}

/**
 * Round and validate one order against its market. Throws
 * `ScriptValidationError` rather than letting a bad number reach the venue.
 */
export const prepareOrder = <T extends { symbol: string; type: OrderType; amount: number }>(
  opts: T,
  { market, exchangeName }: OrderPolicyContext,
): PlaceOrderOpts => {
  const reduceOnly = (opts as { reduceOnly?: boolean }).reduceOnly === true;
  const isReduceOnlyMarket = reduceOnly && opts.type === OrderType.Market;
  const amount = isReduceOnlyMarket
    ? roundMarketOrderSize(opts.amount, market, { reduceOnly: true })
    : roundOrderSize(opts.amount, market, exchangeName);

  // A stop-like order with size 0 is a Full ("infinity") TP/SL covering the
  // whole position. A bare `amount <= 0` guard silently breaks it.
  const infinityStop = opts.amount === 0 && isStopLikeOrderType(opts.type);
  if (infinityStop) {
    if (!exchangeName || !venueSupportsInfinityStops(exchangeName)) {
      throw new ScriptValidationError(`'${exchangeName ?? 'this venue'}' does not support Full (Infinity) TP/SL orders`);
    }
  } else if (!Number.isFinite(amount) || amount <= 0) {
    throw new ScriptValidationError(
      `order size ${opts.amount} rounds to ${amount} for '${opts.symbol}' — below the venue lot step`,
    );
  }

  const roundPriceField = (value: number | undefined, label: string): number | undefined => {
    if (value === undefined) return undefined;
    const rounded = roundOrderPrice(value, market);
    if (!Number.isFinite(rounded) || rounded <= 0) {
      throw new ScriptValidationError(`invalid ${label} for '${opts.symbol}': ${value}`);
    }
    return rounded;
  };

  const source = opts as Record<string, unknown>;
  const out: Record<string, unknown> = { ...source, amount };
  // Every price-bearing field rounds to the venue tick, including a standalone
  // stop's trigger — see CORE_API_VERSION 1.3.0.
  for (const field of ['price', 'stopLoss', 'takeProfit', 'triggerPrice'] as const) {
    const rounded = roundPriceField(source[field] as number | undefined, field);
    if (rounded !== undefined) out[field] = rounded;
  }
  return out as unknown as PlaceOrderOpts;
};

/**
 * The reduce-only clamp. Gated on `autoDownSizeReduceOrders` (hyperliquid,
 * woo): without it a reduce-only order is not shrunk against the reduce-only
 * orders already resting ahead of it, and the venue rejects the excess.
 *
 * Separate from `prepareOrder` because it needs the account's live positions
 * and resting orders, which a caller holding only a market does not have. A
 * caller that cannot supply them skips the clamp rather than clamping against
 * nothing — the venue then rejects the excess, which is what happened before
 * this existed and is strictly better than silently shrinking an order against
 * an empty list.
 */
export const clampReduceOnlyOrder = (
  order: PlaceOrderOpts,
  { market, exchangeName, positions, restingOrders, ticker }: ReduceOnlyClampContext,
): PlaceOrderOpts => {
  if (!order.reduceOnly || !exchangeName) return order;
  if (!venueAutoDownsizesReduceOrders(exchangeName)) return order;
  const price = order.price || ticker?.mark || ticker?.last || 0;
  if (!price) return order;
  return downSizeReduceOrder(market, order, positions as Position[], restingOrders, price);
};
