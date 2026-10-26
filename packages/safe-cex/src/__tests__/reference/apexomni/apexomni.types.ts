import { OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType, Timeframe } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = '6587'; // Tealstreet broker attribution

export const BASE_URL = {
  livenet: 'https://omni.apex.exchange/api',
  testnet: 'https://testnet.omni.apex.exchange/api',
};

export const BASE_WS_URL = {
  public: {
    linear: {
      livenet: 'wss://quote.omni.apex.exchange/realtime_public?v=2&timestamp=1661415017232',
      testnet: 'wss://qa-quote.omni.apex.exchange/realtime_public?v=2&timestamp=1661415017232',
    },
  },
  private: {
    livenet: 'wss://quote.omni.apex.exchange/realtime_private?v=2&timestamp=1661415017232',
    testnet: 'wss://qa-quote.omni.apex.exchange/realtime_private?v=2&timestamp=1661415017233',
  },
};

export const ENDPOINTS = {
  USER: '/v3/user',
  SYMBOLS: '/v3/symbols',
  KLINE: '/v3/klines',
  ACCOUNT: '/v3/account',
  BALANCE: '/v3/account-balance',
  FILLS: '/v3/fills',
  HISTORY_ORDERS: '/v3/history-orders',
  OPEN_ORDERS: '/v3/open-orders',
  CANCEL_ORDERS: '/v3/delete-open-orders',
  CANCEL_ORDER: '/v3/delete-order',
  SET_LEVERAGE: '/v3/set-initial-margin-rate',
  CREATE_ORDER: '/v3/order',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.CREATE_ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.USER,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ACCOUNT, ENDPOINTS.BALANCE];

export const INTERVAL: Record<Timeframe, string> = {
  '1m': '1',
  '3m': '3',
  '5m': '5',
  '15m': '15',
  '30m': '30',
  '1h': '60',
  '2h': '120',
  '4h': '240',
  '6h': '360',
  '12h': '720',
  '1d': 'D',
  '1w': 'W',
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  PENDING: OrderStatus.Open,
  OPEN: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Closed,
  UNTRIGGERED: OrderStatus.Open,
};

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_MARKET: OrderType.StopLoss,
  STOP_LIMIT: OrderType.StopLoss,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TAKE_PROFIT_LIMIT: OrderType.TakeProfit,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const ORDER_TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GOOD_TIL_CANCEL: OrderTimeInForce.GoodTillCancel,
  IMMEDIATE_OR_CANCEL: OrderTimeInForce.ImmediateOrCancel,
  FILL_OR_KILL: OrderTimeInForce.FillOrKill,
  POST_ONLY: OrderTimeInForce.PostOnly,
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  INDEX: OrderStopTriggerType.IndexPrice,
  MARKET: OrderStopTriggerType.LastPrice,
};

export enum ORDER_CANCEL_REASON {
  UNKNOWN_ORDER_CANCEL_REASON = 'UNKNOWN_ORDER_CANCEL_REASON', // Unknown
  EXPIRED = 'EXPIRED', // Order has expired
  USER_CANCELED = 'USER_CANCELED', // User manually canceled order
  COULD_NOT_FILL = 'COULD_NOT_FILL', // Unable to fill FOK/IOC/Post-only orders
  REDUCE_ONLY_CANCELED = 'REDUCE_ONLY_CANCELED', // Unable to fulfil reduce-only orders
  LIQUIDATE_CANCELED = 'LIQUIDATE_CANCELED', // Account or orders triggered liquidation resulting in automatic order cancelation
  INTERNAL_FAILED = 'INTERNAL_FAILED', // Internal processing issues including order matching failure or L2 validation failure
}
