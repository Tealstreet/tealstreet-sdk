import { OrderSide } from '@tealstreet/safe-cex/src/types';

/**
 * Origin of a chaser task - where it was initiated from
 */
export type ChaserOrigin = 'web' | 'cli' | 'webhook' | 'automation';

/**
 * `chase ... to <length>` termination condition. The chaser stops as
 * soon as the live price moves past the resolved threshold in the
 * unfavorable direction (up for buys, down for sells).
 *
 * Three forms:
 *   - `price`   — absolute price threshold (`to 49500`)
 *   - `percent` — percent move from anchor (`to 1%`)
 *   - `usd`     — USD price delta from anchor (`to $50` / `to 50$`)
 *
 * `anchorPrice` is captured at chase start by the caller (typically
 * the CLI command) so percent/usd resolve relative to the price at the
 * moment the user typed the command, not the price at every check.
 * Ignored for `type: 'price'` (the threshold is absolute).
 */
export type ChaserTerminate = {
  type: 'price' | 'percent' | 'usd';
  value: number;
  anchorPrice: number;
};

export type ChaserOrderOptions = {
  symbol: string;
  size: number;
  side: OrderSide;
  maxAttempts: number;
  reduceOnly?: boolean;
  delay: number;
  /** Origin of this chaser - where it was initiated from */
  origin?: ChaserOrigin;
  /** Account name this chaser belongs to - used for filtering in dispose logic */
  accountName?: string;
  /** Optional termination condition for `chase ... to <length>`. */
  terminate?: ChaserTerminate;
  /** Caller's id, echoed on `chaser:complete` so the caller can match its own chase. */
  clientId?: string;
};

/** Stop reason of `removeKeepOrder`: the chase stopped but its order rests on. */
export const CHASER_KEPT_ORDER_STOP_REASON = 'User cancelled, kept order in book';

/** A placement's response was lost and the lookups never found it: it may still land. */
export const CHASER_UNCONFIRMED_ORDER_STOP_REASON = 'Could not confirm order placement';

/**
 * Emitted once on the exchange emitter when a chase ends, however it ends.
 * `filled` is in order-placement units (the unit of `size`), clamped to
 * `[0, size]`; it can undercount a fill that lands during the final cancel.
 */
export type ChaserCompleteEvent = {
  id: string;
  clientId?: string;
  symbol: string;
  side: OrderSide;
  reduceOnly: boolean;
  size: number;
  filled: number;
  stopReason: string;
};
