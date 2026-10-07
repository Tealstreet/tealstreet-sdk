import type { OemsPositionSide, OemsSide } from './context.types';

/**
 * What a planner is allowed to ask for.
 *
 * ## Why intents are higher-level than verbs
 *
 * "Reduce all my positions by 30%" is ONE thing the user said and N orders on
 * a book they cannot see the size of. If the planner had to emit the N orders
 * it would need every position's contract count, every lot step and every
 * position mode in front of it, and it would get the arithmetic wrong on an
 * inverse contract. So it names the INTENT and `resolveOemsPlan` does the
 * arithmetic against live state, deterministically and testably.
 *
 * The split is also what makes the confirm step meaningful: the user is shown
 * the resolved orders, not the sentence the model wrote about them.
 *
 * ## Scopes are filters, never wildcards
 *
 * An empty scope means "everything in the context", which is the reading a
 * user intends by "all my positions". It is safe here only because the
 * resolution is shown and confirmed before anything is sent.
 */

export interface OemsPositionScope {
  /** Account names. Empty means every account in the context. */
  readonly accounts?: readonly string[];
  /** Symbols. Empty means every symbol held. */
  readonly symbols?: readonly string[];
  /** Restrict to one leg. Omitted means both. */
  readonly side?: OemsPositionSide;
}

export interface OemsOrderScope {
  readonly accounts?: readonly string[];
  readonly symbols?: readonly string[];
  readonly side?: OemsSide;
  readonly reduceOnly?: boolean;
}

/** Reduce every matching position by a percentage of what is held. */
export interface OemsReducePositions {
  readonly kind: 'reduce_positions';
  readonly scope: OemsPositionScope;
  /** 0 < percent < 100. A full close is `close_positions`, so that the user
   *  sees the word they said rather than "reduce by 100%". */
  readonly percent: number;
}

export interface OemsClosePositions {
  readonly kind: 'close_positions';
  readonly scope: OemsPositionScope;
}

export interface OemsCancelOrders {
  readonly kind: 'cancel_orders';
  readonly scope: OemsOrderScope;
}

export interface OemsPlaceOrder {
  readonly kind: 'place_order';
  readonly account: string;
  readonly symbol: string;
  readonly side: OemsSide;
  readonly orderType: 'market' | 'limit';
  readonly amount: number;
  /** Required for `limit`, refused for `market`. */
  readonly price?: number;
  readonly reduceOnly?: boolean;
}

/**
 * A protective stop. Its `price` is the TRIGGER and is required — without one
 * the host falls back to whatever the user last typed into the trade panel,
 * and then derives the order type from that price against the mark, so a stop
 * can silently come out a take-profit.
 */
export interface OemsPlaceStopLoss {
  readonly kind: 'place_stop_loss';
  readonly account: string;
  readonly symbol: string;
  readonly side: OemsSide;
  readonly amount: number;
  readonly price: number;
}

/** Exactly one of `price` / `amount`. Both is an amend the user did not mean. */
export interface OemsAmendOrder {
  readonly kind: 'amend_order';
  readonly account: string;
  readonly orderId: string;
  readonly price?: number;
  readonly amount?: number;
}

export interface OemsSetLeverage {
  readonly kind: 'set_leverage';
  readonly account: string;
  readonly symbol: string;
  readonly leverage: number;
  readonly side?: OemsPositionSide;
}

export type OemsIntent =
  | OemsReducePositions
  | OemsClosePositions
  | OemsCancelOrders
  | OemsPlaceOrder
  | OemsPlaceStopLoss
  | OemsAmendOrder
  | OemsSetLeverage;

export type OemsIntentKind = OemsIntent['kind'];

export interface OemsPlan {
  /** What the assistant says in the transcript. Always present — a plan with
   *  no steps is a question or a refusal, not an error. */
  readonly reply: string;
  readonly steps: readonly OemsIntent[];
  /**
   * Reads the model asked for, RAW and unvalidated.
   *
   * They stay raw here on purpose: validating an account name needs the
   * context snapshot, which `parseOemsPlan` does not take. `parseReadCalls`
   * does that at the point of use, in the browser that is about to execute
   * them against live credentials.
   */
  readonly reads: readonly unknown[];
}
