import { OrderTimeInForce } from '../../../types';

export const RECV_WINDOW = 5_000;

// Binance's options docs name demo-fapi.binance.com, which answers 403 on /eapi; the old futures testnet serves it.
export const BASE_URL = {
  livenet: 'https://eapi.binance.com',
  testnet: 'https://testnet.binancefuture.com',
};

export const ENDPOINTS = {
  MARKETS: '/eapi/v1/exchangeInfo',
  TICKERS: '/eapi/v1/ticker',
  MARK: '/eapi/v1/mark',
  KLINE: '/eapi/v1/klines',
  ACCOUNT: '/eapi/v1/marginAccount',
  POSITIONS: '/eapi/v1/position',
  ORDER: '/eapi/v1/order',
  OPEN_ORDERS: '/eapi/v1/openOrders',
  CANCEL_SYMBOL_ORDERS: '/eapi/v1/allOpenOrders',
  HISTORY_ORDERS: '/eapi/v1/historyOrders',
  USER_TRADES: '/eapi/v1/userTrades',
};

export const PUBLIC_ENDPOINTS = [ENDPOINTS.MARKETS, ENDPOINTS.TICKERS, ENDPOINTS.MARK, ENDPOINTS.KLINE];

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.ORDER, ENDPOINTS.CANCEL_SYMBOL_ORDERS];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ACCOUNT, ENDPOINTS.POSITIONS, ENDPOINTS.OPEN_ORDERS];

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
  GTX: OrderTimeInForce.PostOnly,
};
