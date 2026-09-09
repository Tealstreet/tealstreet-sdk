import type { ExchangeName } from '../../types';

import { OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType, Timeframe } from '../../types';
import { getPublicCorsProxy } from '../../utils/transportResolution';

export const RECV_WINDOW = 20_000;

export type LighterDeploymentNetwork = 'livenet' | 'testnet' | 'robinhood' | 'robinhood-testnet';

export type LighterDeploymentProfile = {
  network: LighterDeploymentNetwork;
  apiBaseUrl: string;
  wsUrl: string;
  signerChainId: number;
  quoteCurrency: 'USDC' | 'USDG';
  usesRestProxy: boolean;
  supportsWsProxy: boolean;
};

export const LIGHTER_DEPLOYMENT_PROFILES: Record<LighterDeploymentNetwork, LighterDeploymentProfile> = {
  livenet: {
    network: 'livenet',
    apiBaseUrl: 'https://mainnet.zklighter.elliot.ai',
    wsUrl: 'wss://mainnet.zklighter.elliot.ai/stream',
    signerChainId: 304,
    quoteCurrency: 'USDC',
    usesRestProxy: true,
    supportsWsProxy: true,
  },
  testnet: {
    network: 'testnet',
    apiBaseUrl: 'https://testnet-api.lighter.xyz',
    wsUrl: 'wss://testnet.zklighter.elliot.ai/stream',
    signerChainId: 300,
    quoteCurrency: 'USDC',
    usesRestProxy: true,
    supportsWsProxy: false,
  },
  robinhood: {
    network: 'robinhood',
    apiBaseUrl: 'https://api.rh.lighter.xyz',
    wsUrl: 'wss://api.rh.lighter.xyz/stream',
    signerChainId: 466324,
    quoteCurrency: 'USDG',
    usesRestProxy: true,
    supportsWsProxy: true,
  },
  'robinhood-testnet': {
    network: 'robinhood-testnet',
    apiBaseUrl: 'https://api.rh-testnet.lighter.xyz',
    wsUrl: 'wss://api.rh-testnet.lighter.xyz/stream',
    signerChainId: 300,
    quoteCurrency: 'USDG',
    usesRestProxy: false,
    supportsWsProxy: false,
  },
};

export const isLighterRobinhoodExchange = (exchangeName?: ExchangeName): boolean => exchangeName === 'lighter-robinhood';

export const resolveLighterDeploymentNetwork = (params: {
  exchangeName?: ExchangeName;
  testnet?: boolean;
}): LighterDeploymentNetwork => {
  if (isLighterRobinhoodExchange(params.exchangeName)) {
    return params.testnet ? 'robinhood-testnet' : 'robinhood';
  }

  return params.testnet ? 'testnet' : 'livenet';
};

export const resolveLighterDeploymentProfile = (params: {
  exchangeName?: ExchangeName;
  testnet?: boolean;
}): LighterDeploymentProfile => {
  return LIGHTER_DEPLOYMENT_PROFILES[resolveLighterDeploymentNetwork(params)];
};

// Resolved at CALL time (not a module-eval const) so the public CORS host is never
// baked into a module-eval string.
export const getBaseUrl = (net: LighterDeploymentNetwork): string =>
  LIGHTER_DEPLOYMENT_PROFILES[net].usesRestProxy
    ? `${getPublicCorsProxy()}/${LIGHTER_DEPLOYMENT_PROFILES[net].apiBaseUrl}`
    : LIGHTER_DEPLOYMENT_PROFILES[net].apiBaseUrl;

export const getLighterApiBaseUrl = (params: { exchangeName?: ExchangeName; testnet?: boolean }): string =>
  getBaseUrl(resolveLighterDeploymentNetwork(params));

export const getLighterWsUrl = (params: { exchangeName?: ExchangeName; testnet?: boolean }): string =>
  resolveLighterDeploymentProfile(params).wsUrl;

export const ENDPOINTS = {
  ACCOUNT: '/api/v1/account',
  ACCOUNTS_BY_L1_ADDRESS: '/api/v1/accountsByL1Address',
  ACCOUNT_LIMITS: '/api/v1/accountLimits',
  CHANGE_ACCOUNT_TIER: '/api/v1/changeAccountTier',
  MARKETS: '/api/v1/orderBooks',
  ORDER_BOOK_DETAILS: '/api/v1/orderBookDetails',
  EXCHANGE_STATS: '/api/v1/exchangeStats',
  CANDLES: '/api/v1/candles',
  TRADES: '/api/v1/trades',
  OPEN_ORDERS: '/api/v1/accountActiveOrders',
  INACTIVE_ORDERS: '/api/v1/accountInactiveOrders',
  SEND_TX: '/api/v1/sendTx',
  SEND_TX_BATCH: '/api/v1/sendTxBatch',
};

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.SEND_TX, ENDPOINTS.SEND_TX_BATCH];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ACCOUNT, ENDPOINTS.ACCOUNT_LIMITS, ENDPOINTS.OPEN_ORDERS];

export const INTERVAL: Partial<Record<Timeframe, string>> = {
  '1m': '1m',
  '5m': '5m',
  '15m': '15m',
  '30m': '30m',
  '1h': '1h',
  '4h': '4h',
  '12h': '12h',
  '1d': '1d',
};

export const resolveLighterInterval = (interval: Timeframe): string => {
  const resolution = INTERVAL[interval];
  if (!resolution) {
    throw new Error(
      `Lighter does not support raw ${interval} candles. Supported resolutions: 1m, 5m, 15m, 30m, 1h, 4h, 12h, 1d.`,
    );
  }
  return resolution;
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  PENDING: OrderStatus.Open,
  OPEN: OrderStatus.Open,
  PARTIALLY_FILLED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELLED: OrderStatus.Canceled,
  CANCELED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Closed,
  REJECTED: OrderStatus.Canceled,
};

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_LOSS: OrderType.StopLoss,
  STOP_MARKET: OrderType.StopLoss,
  STOP_LIMIT: OrderType.StopLoss,
  TAKE_PROFIT: OrderType.TakeProfit,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TAKE_PROFIT_LIMIT: OrderType.TakeProfit,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const ORDER_TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
  POST_ONLY: OrderTimeInForce.PostOnly,
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  INDEX: OrderStopTriggerType.IndexPrice,
  MARK: OrderStopTriggerType.MarkPrice,
  LAST: OrderStopTriggerType.LastPrice,
};
