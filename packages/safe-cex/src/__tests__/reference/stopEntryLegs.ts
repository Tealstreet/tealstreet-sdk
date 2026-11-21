import type { PlaceOrderOpts } from '../../types';

import { OrderType } from '../../types';

/** AGT-115: mirrors the Rust engine's `PlaceOrderOpts::stop_entry_with_attached_legs`. */
export const stopEntryWithAttachedLegs = (order: PlaceOrderOpts) =>
  order.type !== OrderType.Limit &&
  order.type !== OrderType.Market &&
  [order.stopLoss, order.takeProfit].some((leg) => leg != null && leg !== 0);

/** The Rust engine's `stop_entry_legs_refusal` copy, so both paths say the same thing. */
export const stopEntryLegsRefusal = (venue: string) =>
  `Attaching a stop-loss or take-profit to a stop entry isn't supported on ${venue}, so nothing was sent. Place the stop-loss after the entry fills.`;
