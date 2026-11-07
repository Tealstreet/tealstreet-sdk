import type { Timeframe } from '../../../types';

import { OrderStatus, OrderTimeInForce, OrderType } from '../../../types';

export const KUCOIN_SPOT_RECV_WINDOW = 20_000;
export const KUCOIN_SPOT_BROKER_ID = '3zexp';
export const KUCOIN_SPOT_BASE_URL = 'https://api.kucoin.com';

export const KUCOIN_SPOT_ENDPOINTS = {
  ANNOUNCEMENTS: '/api/v3/announcements',
  ACCOUNTS: '/api/v1/accounts',
  ACCOUNT_SUMMARY: '/api/v2/user-info',
  MARKETS: '/api/v2/symbols',
  TICKERS: '/api/v1/market/allTickers',
  KLINE: '/api/v1/market/candles',
  OPEN_ORDERS: '/api/v1/hf/orders/active/page',
  ACTIVE_ORDER_SYMBOLS: '/api/v1/hf/orders/active/symbols',
  PLACE_ORDER: '/api/v1/hf/orders',
  CANCEL_ORDER: '/api/v1/hf/orders',
  CANCEL_SYMBOL_ORDERS: '/api/v1/hf/orders',
  CANCEL_ALL_ORDERS: '/api/v1/hf/orders/cancelAll',
  MODIFY_ORDER: '/api/v1/hf/orders/alter',
  CLOSED_ORDERS: '/api/v1/hf/orders/done',
  FILLS: '/api/v1/hf/fills',
  GET_PUBLIC_WS_TOKEN: '/api/v1/bullet-public',
  GET_PRIVATE_WS_TOKEN: '/api/v1/bullet-private',
};

export const KUCOIN_SPOT_PUBLIC_ENDPOINTS = [
  KUCOIN_SPOT_ENDPOINTS.ANNOUNCEMENTS,
  KUCOIN_SPOT_ENDPOINTS.MARKETS,
  KUCOIN_SPOT_ENDPOINTS.TICKERS,
  KUCOIN_SPOT_ENDPOINTS.KLINE,
  KUCOIN_SPOT_ENDPOINTS.GET_PUBLIC_WS_TOKEN,
];

export const KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY = [
  KUCOIN_SPOT_ENDPOINTS.PLACE_ORDER,
  KUCOIN_SPOT_ENDPOINTS.CANCEL_ORDER,
  KUCOIN_SPOT_ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  KUCOIN_SPOT_ENDPOINTS.CANCEL_ALL_ORDERS,
  KUCOIN_SPOT_ENDPOINTS.MODIFY_ORDER,
];

export const KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC = [
  KUCOIN_SPOT_ENDPOINTS.ACCOUNTS,
  KUCOIN_SPOT_ENDPOINTS.OPEN_ORDERS,
  KUCOIN_SPOT_ENDPOINTS.CLOSED_ORDERS,
  KUCOIN_SPOT_ENDPOINTS.FILLS,
];

export const KUCOIN_SPOT_INTERVAL: Record<Timeframe, string> = {
  '1m': '1min',
  '3m': '3min',
  '5m': '5min',
  '15m': '15min',
  '30m': '30min',
  '1h': '1hour',
  '2h': '2hour',
  '4h': '4hour',
  '6h': '6hour',
  '12h': '12hour',
  '1d': '1day',
  '1w': '1week',
};

export const KUCOIN_SPOT_ORDER_STATUS: Record<string, OrderStatus> = {
  active: OrderStatus.Open,
  open: OrderStatus.Open,
  new: OrderStatus.Open,
  match: OrderStatus.Open,
  done: OrderStatus.Closed,
  filled: OrderStatus.Closed,
  canceled: OrderStatus.Canceled,
  cancelled: OrderStatus.Canceled,
};

export const KUCOIN_SPOT_ORDER_TYPE: Record<string, OrderType> = {
  limit: OrderType.Limit,
  market: OrderType.Market,
};

export const KUCOIN_SPOT_TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
};

export type KucoinSpotWsServer = {
  endpoint: string;
  encrypt: boolean;
  protocol: string;
  pingInterval: number;
  pingTimeout: number;
};

export type KucoinSpotWsTokenResponse = {
  code: string;
  data?: {
    token: string;
    instanceServers: KucoinSpotWsServer[];
  };
  msg?: string;
};
