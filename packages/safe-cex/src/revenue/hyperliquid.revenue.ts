import type { RevenueResult, VenueRevenueFn } from './types';

import { TEALSTREET_BUILDER_FEE } from '../exchanges/hyperliquid/hyperliquid.constants';

/** `TEALSTREET_BUILDER_FEE` is tenths of a basis point, so 10 is 1bp is 0.0001. */
export const HYPERLIQUID_BUILDER_RATE = TEALSTREET_BUILDER_FEE / 100_000;

/**
 * The reference implementation, and the only venue where the answer is this short.
 *
 * We are the builder, we set the rate, and it does not vary by tier or by whether
 * the user came through a referral — so the whole function is notional times 1bp,
 * with one real branch.
 *
 * **SPOT PAYS NOTHING**, and that is measured rather than assumed: across all 233
 * published days the venue charged no builder fee on any spot fill (`@260` on
 * 2026-08-22, `@334` on 08-24, `@142` on 09-01 — 7 fills, $44.23 of notional,
 * $0.00 collected). Returning a computed 1bp for those would invent revenue that
 * was never received, and the daily reconciliation would then fail against the
 * venue's own numbers.
 */
export const hyperliquidRevenue: VenueRevenueFn<'hyperliquid'> = ({ fill, market }): RevenueResult => {
  if (market.isSpot) {
    return { kind: 'none', reason: 'venue-pays-nothing' };
  }

  const notional = fill.price * fill.qty;
  if (!Number.isFinite(notional) || notional <= 0) {
    return { kind: 'unknown', reason: 'fill carries no usable notional' };
  }

  return {
    kind: 'earned',
    amount: notional * HYPERLIQUID_BUILDER_RATE,
    // Builder fees settle in the quote asset the fee was charged in.
    asset: fill.commissionAsset ?? 'USDC',
    basis: 'computed_from_notional',
  };
};

/**
 * Hyperliquid is the one venue that ALSO reports what we earned, per fill
 * (`builderFee` on `userFills`). That makes it the only place the function above
 * can be checked against the venue rather than against itself — so a drift in
 * our arithmetic, our rate constant, or the venue's own behaviour surfaces on a
 * single fill instead of at the end of a payout period.
 *
 * The tolerance is per-fill because the feed truncates each fee to six decimals.
 */
export const verifyAgainstVenue = (
  computed: RevenueResult,
  venueReported: number | undefined,
): { readonly agrees: boolean; readonly reason?: string } => {
  if (venueReported === undefined) return { agrees: true };

  if (computed.kind !== 'earned') {
    // The venue paid us and we decided we earned nothing. Always wrong, and the
    // direction that silently under-pays an affiliate.
    return venueReported > 0
      ? { agrees: false, reason: `venue reported ${venueReported} but we computed ${computed.kind}` }
      : { agrees: true };
  }

  const drift = Math.abs(computed.amount - venueReported);
  return drift <= 1e-6
    ? { agrees: true }
    : { agrees: false, reason: `computed ${computed.amount} vs venue ${venueReported}` };
};
