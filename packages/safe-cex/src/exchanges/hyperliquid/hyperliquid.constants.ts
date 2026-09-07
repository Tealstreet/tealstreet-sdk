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
