// see https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
export const singularSubscriptionTypes: readonly string[] = ['allMids'];
export const systemSubscriptionTypes: readonly string[] = [
  'notification',
  'webData2',
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
