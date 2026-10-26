// see https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
export const singularSubscriptionTypes: readonly string[] = ['allMids'];
export const systemSubscriptionTypes: readonly string[] = [
  'notification',
  'spotState',
  'userFills',
  'userNonFundingLedgerUpdates',
];

export const MAINNET_API_URL = 'https://api.hyperliquid.xyz';
