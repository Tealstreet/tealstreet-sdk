import type { PlaceOrderOpts } from '../../types';

import { OrderType } from '../../types';

/** AGT-115: mirrors the Rust engine's `PlaceOrderOpts::stop_entry_with_attached_legs`. */
export const stopEntryWithAttachedLegs = (order: PlaceOrderOpts) =>
  order.type !== OrderType.Limit &&
  order.type !== OrderType.Market &&
  [order.stopLoss, order.takeProfit].some((leg) => leg != null && leg !== 0);

/** The Rust engine's `stop_entry_legs_refusal` copy, so both paths say the same thing. */
export const stopEntryLegsRefusal = (venue: string) =>
  `Attaching a stop-loss or take-profit to a stop entry isn't supported on ${venue}. Place the stop-loss after the entry fills.`;

/** AGT-120: mirrors the Rust engine's `PlaceOrderOpts::reduce_only_with_attached_legs`. */
export const reduceOnlyWithAttachedLegs = (order: PlaceOrderOpts) =>
  (order.reduceOnly === true || order.close === true) &&
  [order.stopLoss, order.takeProfit].some((leg) => leg != null && leg !== 0);

/** The Rust engine's `REDUCE_ONLY_LEGS_REFUSAL`, which it applies before any venue sees the order. */
export const REDUCE_ONLY_LEGS_REFUSAL =
  "A stop-loss or take-profit can't be attached to a reduce-only order. Place it as its own order.";
