import type { Execution, Market } from '../types';

/**
 * What Tealstreet earned on a fill.
 *
 * ---------------------------------------------------------------------------
 * THE VENUE DOES NOT TELL US THIS, AND THAT IS THE NORMAL CASE
 * ---------------------------------------------------------------------------
 *
 * A fill's `commission` is what the USER paid the exchange. Of the twelve venues
 * we attach broker attribution to, exactly ONE — hyperliquid — also reports what
 * we earned. Everywhere else our cut is a FUNCTION of the fill:
 *
 *     f(fee paid, maker/taker, market category, the user's affiliate status,
 *       our terms with that venue, the date) -> our cut
 *
 * So this module is a contract, not an implementation. Adding a venue is writing
 * one function and registering it.
 *
 * ---------------------------------------------------------------------------
 * THE SIGNATURE IS SPLIT IN TWO, AND THAT IS THE WHOLE DESIGN
 * ---------------------------------------------------------------------------
 *
 * A single wide context with optional fields does not survive twelve venues: it
 * becomes thirty optional properties, most irrelevant to any given venue, and
 * the type system stops helping — a venue's function that needs a fee tier
 * compiles perfectly when the caller never passed one, and fails at runtime.
 *
 * The inputs are really two kinds:
 *
 *   * Derivable from the fill and its market — fee, asset, maker/taker,
 *     notional, category, time. Bounded by what a fill IS, so it never grows.
 *     That is `RevenueContext`'s fixed half.
 *   * Venue-specific commercial and account facts — referred-by-us, tiers,
 *     negotiated share. This is the part that varies, so it is typed PER VENUE
 *     through `VenueRevenueInputs` and reaches the function as `inputs`.
 *
 * Adding bybit therefore adds one entry to `VenueRevenueInputs` and touches no
 * other venue, and whoever calls it must produce that entry or fail to compile.
 *
 * Numbers stay out of the mirror. safe-cex ships publicly, and a negotiated
 * rev-share is not ours to publish, so the SHAPE of the decision lives in the
 * venue's function while the numbers arrive in its `inputs`. Hyperliquid's 1bp
 * is the exception that stays inline — we disclose it to the user when they
 * approve the builder fee.
 */

/**
 * How a figure was arrived at. Not decoration: a number the venue reported and
 * one we derived from our own contract are different claims, and only the first
 * can be reconciled against anything.
 */
export type RevenueBasis =
  /** The venue reported what we earned on this fill. Only hyperliquid today. */
  | 'venue_reported'
  /** Derived from the fee the user paid times our share. */
  | 'computed_from_fee'
  /** Derived from notional times a rate we set. */
  | 'computed_from_notional';

/**
 * **`unknown` is not zero, and keeping them apart is the point.**
 *
 * A venue with no implemented function must be loudly unimplemented. Collapsing
 * it to `earned: 0` states that an affiliate is owed nothing, which is
 * indistinguishable from a user who did not trade, and nobody ever finds out.
 * The same distinction recurs across this system — a missing day vs an empty
 * one, an unlinked address vs an unpaid one, an excluded account vs an idle one
 * — and it has been got wrong at least once in each.
 */
export type RevenueResult =
  | { readonly kind: 'earned'; readonly amount: number; readonly asset: string; readonly basis: RevenueBasis }
  /** Confident that nothing was earned, and why. */
  | { readonly kind: 'none'; readonly reason: 'not-ours' | 'venue-pays-nothing' }
  /** We cannot say. Never render as zero; never pay on it. */
  | { readonly kind: 'unknown'; readonly reason: string };

/**
 * The venue-specific inputs each function requires.
 *
 * **Declaration-merged**, so a venue's own directory can declare what it needs
 * without editing a central file that accumulates every venue's concerns:
 *
 *     declare module '../revenue/types' {
 *       interface VenueRevenueInputs {
 *         bybit: { referredByUs: boolean; category: BybitCategory; brokerTier: string };
 *       }
 *     }
 *
 * A key here with no registered function is a COMPILE error — see
 * `VenueRevenueRegistry` — so wiring a venue up is not something you can forget.
 */
export interface VenueRevenueInputs {
  /** Needs nothing beyond the fill: we set the rate, and it does not vary. */
  hyperliquid: Record<string, never>;
}

export type RevenueVenue = keyof VenueRevenueInputs;

export type RevenueContext<TInputs> = {
  readonly fill: Execution;
  /** Category branching (spot vs linear vs inverse) and the notional. */
  readonly market: Market;
  /**
   * When to evaluate at, ms. Terms change, and a fill from March must be valued
   * at March's terms — the same rule that stamps affiliate rates onto a ledger
   * row so a later change cannot rewrite what someone was already told they earned.
   */
  readonly at: number;
  readonly inputs: TInputs;
};

export type VenueRevenueFn<K extends RevenueVenue> = (ctx: RevenueContext<VenueRevenueInputs[K]>) => RevenueResult;

/**
 * Every venue declared in `VenueRevenueInputs` must have a function. The mapped
 * type is what makes that true: declaring a venue and forgetting to register it
 * does not compile.
 */
export type VenueRevenueRegistry = { readonly [K in RevenueVenue]: VenueRevenueFn<K> };
