import { OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType, Timeframe } from '../../types';
import { PUBLIC_CORS_PROXY } from '../../utils/transportResolution';

export const RECV_WINDOW = 20_000;

export const BASE_URL = {
  livenet: `${PUBLIC_CORS_PROXY}/https://mainnet.zklighter.elliot.ai`,
  testnet: `${PUBLIC_CORS_PROXY}/https://testnet-api.lighter.xyz`,
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://mainnet.zklighter.elliot.ai/stream',
    testnet: 'wss://testnet.zklighter.elliot.ai/stream',
  },
  private: {
    livenet: 'wss://mainnet.zklighter.elliot.ai/stream',
    testnet: 'wss://testnet.zklighter.elliot.ai/stream',
  },
};

export const ENDPOINTS = {
  // Account endpoints
  ACCOUNT: '/api/v1/account',
  ACCOUNTS_BY_L1_ADDRESS: '/api/v1/accountsByL1Address',
  ACCOUNT_LIMITS: '/api/v1/accountLimits',
  CHANGE_ACCOUNT_TIER: '/api/v1/changeAccountTier',
  ACCOUNT_BALANCE: '/api/v1/account/balance',
  ACCOUNT_NONCE: '/api/v1/account/nonce',
  API_KEYS: '/api/v1/account/api-keys',

  // Market data endpoints
  MARKETS: '/api/v1/orderBooks',
  ORDER_BOOK_DETAILS: '/api/v1/orderBookDetails',
  TICKERS: '/api/v1/tickers',
  TICKER: '/api/v1/ticker',
  KLINES: '/api/v1/klines',
  ORDERBOOK: '/api/v1/orderbook',
  TRADES: '/api/v1/trades',

  // Order endpoints
  ORDERS: '/api/v1/orders',
  OPEN_ORDERS: '/api/v1/accountActiveOrders',
  INACTIVE_ORDERS: '/api/v1/accountInactiveOrders',
  ORDER_HISTORY: '/api/v1/orders/history',
  CREATE_ORDER: '/api/v1/orders',
  CANCEL_ORDER: '/api/v1/orders',
  CANCEL_ALL_ORDERS: '/api/v1/orders/all',

  // Position endpoints
  POSITIONS: '/api/v1/positions',

  // Transaction endpoints
  TRANSACTIONS: '/api/v1/transactions',
};

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.CREATE_ORDER, ENDPOINTS.CANCEL_ORDER, ENDPOINTS.CANCEL_ALL_ORDERS];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.ACCOUNT_BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.OPEN_ORDERS,
];

export const INTERVAL: Record<Timeframe, string> = {
  '1m': '1m',
  '3m': '3m',
  '5m': '5m',
  '15m': '15m',
  '30m': '30m',
  '1h': '1h',
  '2h': '2h',
  '4h': '4h',
  '6h': '6h',
  '12h': '12h',
  '1d': '1d',
  '1w': '1w',
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
