import type { ExchangeName, ExchangeOptions } from '../types';
import type { RevenueVenue } from './types';

import { isRevenueVenue } from './resolve';

/**
 * Exchanges that trade on a venue we can earn from, folded to that venue.
 *
 * An ALIAS layer over `isRevenueVenue`, never a second source of truth: an
 * exchange whose own name is already a revenue venue maps to itself, and this
 * map names only the ones whose name differs. `hyperliquidspot` is here because
 * it is the SAME WALLET as the perp account even though spot pays us nothing —
 * an identity is worth recording wherever we see it, and what that identity
 * earns is `f`'s problem rather than this file's.
 */
const VENUE_ALIASES: Partial<Record<ExchangeName, RevenueVenue>> = {
  hyperliquidspot: 'hyperliquid',
};

export const revenueVenueOf = (exchange: ExchangeName): RevenueVenue | undefined =>
  isRevenueVenue(exchange) ? exchange : VENUE_ALIASES[exchange];

const trimmed = (value: string | undefined): string | undefined => {
  const next = value?.trim();
  return next ? next : undefined;
};

type VenueAccountReader = (options: ExchangeOptions) => string | undefined;

/**
 * The venue-side id of a live account: the address whose fills we actually hold.
 *
 * Hyperliquid reads `vault ?? applicationId` because that is the exact `user`
 * every info query is scoped to, so a claim can never disagree with the rows it
 * attributes — if that pair is wrong for an account, the account's stored fills
 * are wrong in the same direction, and attribution inherits the bug instead of
 * inventing a second one.
 *
 * `applicationId` is RESOLVED during `start()` (an agent or subaccount key
 * becomes its master), so before startup finishes there is no answer. That is
 * why an unresolved account yields `undefined` rather than falling back to
 * `options.key` — the key of an agent wallet is not a trading address, and
 * claiming it would link a user to an address that never appears in the feed.
 *
 * Mapped over `RevenueVenue` for the same reason the revenue registry is:
 * declaring a venue without teaching this file how to name its accounts must
 * fail to compile, not fail at runtime on the one venue nobody tested.
 */
const VENUE_ACCOUNT_READERS: { readonly [K in RevenueVenue]: VenueAccountReader } = {
  hyperliquid: (options) => trimmed(options.vault) ?? trimmed(options.applicationId),
};

/**
 * What a venue-side account id may look like.
 *
 * First claim wins, and there is no signature challenge, so the ONLY thing
 * standing between a claim and someone else's wallet is that the claim has to
 * be a real address of the right shape and that a contested one pays nobody.
 * A format check is cheap and it removes the whole class of junk, typo and
 * probe claims — an id that cannot appear in the feed can never be attributed,
 * so storing it is pure surface.
 *
 * Mapped over `RevenueVenue` for the same reason the readers are: a new venue
 * must decide what its account ids look like, at compile time.
 */
const VENUE_ACCOUNT_FORMATS: { readonly [K in RevenueVenue]: RegExp } = {
  // A 20-byte EVM address, lowercased — the exact shape
  // `hyperliquid_builder_fills.trader_address` is stored in.
  hyperliquid: /^0x[0-9a-f]{40}$/,
};

export const isValidVenueAccountId = (venue: RevenueVenue, venueAccountId: string): boolean =>
  VENUE_ACCOUNT_FORMATS[venue].test(venueAccountId);

export type VenueAccount = {
  readonly venue: RevenueVenue;
  readonly venueAccountId: string;
};

/**
 * Lowercased, because `hyperliquid_builder_fills.trader_address` is stored
 * lowercased and a mixed-case claim would join to nothing while looking linked.
 */
export const resolveVenueAccount = (
  exchange: ExchangeName,
  options: ExchangeOptions,
): VenueAccount | undefined => {
  const venue = revenueVenueOf(exchange);
  if (!venue) return undefined;

  const venueAccountId = VENUE_ACCOUNT_READERS[venue](options)?.toLowerCase();
  if (!venueAccountId || !isValidVenueAccountId(venue, venueAccountId)) return undefined;

  return { venue, venueAccountId };
};
