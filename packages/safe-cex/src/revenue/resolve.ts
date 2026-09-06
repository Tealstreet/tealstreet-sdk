import type { ExchangeName } from '../types';
import type { RevenueContext, RevenueResult, RevenueVenue, VenueRevenueInputs, VenueRevenueRegistry } from './types';

import { hyperliquidRevenue } from './hyperliquid.revenue';

/**
 * One entry per venue whose function is written.
 *
 * We attach broker attribution on TWELVE venues today — bybit, binance, okx,
 * bitget, blofin, hyperliquid, lighter, asterdex, apexomni, paradex, grvt and
 * orderly — so revenue is presumably accruing on all of them and we read none of
 * it back. The eleven absent here are not "zero", they are unwritten, and
 * `resolveVenueRevenue` says exactly that.
 */
export const VENUE_REVENUE: VenueRevenueRegistry = {
  hyperliquid: hyperliquidRevenue,
};

/** Narrows a runtime `ExchangeName` to a venue that has a function. */
export const isRevenueVenue = (exchange: ExchangeName): exchange is RevenueVenue =>
  Object.prototype.hasOwnProperty.call(VENUE_REVENUE, exchange);

export const resolveVenueRevenue = <K extends RevenueVenue>(
  exchange: K,
  ctx: RevenueContext<VenueRevenueInputs[K]>,
): RevenueResult => VENUE_REVENUE[exchange](ctx);

/**
 * For a caller iterating fills across mixed venues, where the exchange is a
 * runtime value. Returns `unknown` rather than throwing: an unimplemented venue
 * is a normal state of this system, not an error, and it must be recorded as
 * "we cannot say" instead of skipped.
 */
export const revenueForUnknownVenue = (exchange: ExchangeName): RevenueResult => ({
  kind: 'unknown',
  reason: `no revenue function implemented for ${exchange}`,
});

export const venuesWithRevenueFn = (): RevenueVenue[] => Object.keys(VENUE_REVENUE) as RevenueVenue[];
