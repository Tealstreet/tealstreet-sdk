import type { ExchangeOptions } from '../../types';

// see https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
export const singularSubscriptionTypes: readonly string[] = ['allMids'];
export const systemSubscriptionTypes: readonly string[] = [
  'notification',
  'orderUpdates',
  'userEvents',
  'userFills',
  'userFundings',
  'userNonFundingLedgerUpdates',
];

export const MAINNET_API_URL = 'https://api.hyperliquid.xyz';

// Tealstreet builder wallet addresses
export const TEALSTREET_BUILDER_ADDRESS_MAINNET = '0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d';
export const TEALSTREET_BUILDER_ADDRESS_TESTNET = '0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba';

// Helper to get the right address based on network
export const getBuilderAddress = (testnet: boolean): string =>
  testnet ? TEALSTREET_BUILDER_ADDRESS_TESTNET : TEALSTREET_BUILDER_ADDRESS_MAINNET;

// Builder fee in tenths of basis points (10 = 1bp = 0.01%)
export const TEALSTREET_BUILDER_FEE = 10;

/**
 * The whole ladder, keyed by the `f` we send: 10/30/50 tenths of a bp.
 *
 * **`venue_builder_tiers` on the server is the authority; this is the browser's
 * copy, and it exists for exactly one job** — the standalone approval page runs
 * with no session and no atoms, so it takes its builder from the URL, and an
 * unvalidated address there would let anyone send a link asking a user to sign
 * away 5bp to a wallet of their choosing. A closed allowlist makes that a
 * broken link instead of a phishing page.
 *
 * Mainnet only, like the ladder itself. These are on-chain addresses we already
 * disclose to every user at approval time, so nothing here is private.
 */
export const HYPERLIQUID_BUILDER_TIERS: Readonly<Record<number, string>> = {
  10: TEALSTREET_BUILDER_ADDRESS_MAINNET,
  30: '0x95d699952Fd9B110ef1218369def8538CCbB094B',
  50: '0xbB7b32512e1aDadE56f2B7f092ED0A232BE80D66',
};

/**
 * Is this (builder, rate) pair one of ours? Case-insensitive, because an
 * address survives a round trip through a URL, a database and a signature in
 * whatever case each of them prefers.
 */
export const isKnownHyperliquidBuilder = (address: string, feeTenthsBps: number): boolean =>
  // `hasOwn` rather than a bare index, because the caller's rate comes off a
  // URL. A number can never spell `__proto__`, but a lookup that consults the
  // prototype chain at all is not the thing to leave load-bearing on a page
  // whose only job is collecting a signature.
  Object.hasOwn(HYPERLIQUID_BUILDER_TIERS, feeTenthsBps) &&
  HYPERLIQUID_BUILDER_TIERS[feeTenthsBps]?.toLowerCase() === address.trim().toLowerCase();

/**
 * Which builder wallet an account routes through, and at what rate.
 *
 * The two are ONE fact. A user's approval on Hyperliquid is per (builder, max
 * rate), so an address paired with a rate the user did not approve for THAT
 * address is an order the venue rejects — which surfaces as "orders stopped
 * working", not as a fee problem. Passing them as a pair is what makes that
 * pairing hard to break.
 */
export type HyperliquidBuilderConfig = {
  readonly address: string;
  readonly feeTenthsBps: number;
};

/**
 * Tenths of a basis point to the percentage string Hyperliquid signs and
 * stores: 10 -> `0.01%`, 30 -> `0.03%`.
 *
 * **The approval signature and the approval CHECK have to come from this one
 * function.** The signed `maxFeeRate` used to be an independent literal, so
 * making only the check configurable would have had a 3bp user sign `0.01%`,
 * the venue record 10, the check demand 30, and the approval prompt reappear
 * forever with no way out.
 *
 * **Only exact on whole basis points, i.e. multiples of 10.** 10/30/50 format
 * to `0.01%`/`0.03%`/`0.05%` precisely, but 7 gives
 * `0.006999999999999999%` — this is IEEE-754, not a bug to route around. The
 * string is SIGNED by the user and compared by the venue, so a tier that is not
 * a whole basis point would have the artifact baked into a signature. Keep the
 * ladder in whole bp.
 */
export const builderFeeRateFromTenthsBps = (feeTenthsBps: number): string => `${feeTenthsBps / 10 / 100}%`;

/**
 * The builder for an account, defaulting to the historical single wallet at
 * 1bp. Every read goes through here so the fallback exists once — a second copy
 * is how one call site keeps charging the old rate after the others move.
 *
 * Type-only import of `ExchangeOptions`, so this file stays a leaf and can go
 * on being imported by the revenue module and by serverless routes.
 */
export const resolveHyperliquidBuilder = (
  options: Pick<ExchangeOptions, 'hyperliquidBuilder' | 'testnet'> | undefined,
): HyperliquidBuilderConfig =>
  options?.hyperliquidBuilder ?? {
    address: getBuilderAddress(options?.testnet ?? false),
    feeTenthsBps: TEALSTREET_BUILDER_FEE,
  };

/**
 * Hyperliquid addresses spot pairs from the same asset-index space as perps,
 * shifted by 10000. Core perps occupy `0..N`; HIP-3 dexes start at 110000
 * (see `HIP3_ASSET_INDEX_BASE` usage in the exchange class). Because the three
 * ranges are disjoint, a single exchange instance can carry perp, spot, and
 * HIP-3 markets in one store and still build a correct order wire from
 * `market.info.hyperliquidId`.
 */
export const SPOT_ASSET_INDEX_OFFSET = 10000;

/** Max decimal places Hyperliquid accepts in a price wire, per market kind. */
export const PERP_MAX_PRICE_DECIMALS = 6;
export const SPOT_MAX_PRICE_DECIMALS = 8;
