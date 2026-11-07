import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide } from '../../../types';

export const BASE_URL = {
  livenet: 'https://futures.kraken.com/derivatives/api',
  testnet: 'https://demo-futures.kraken.com/derivatives/api',
};

/**
 * The HISTORY base, which is a different path prefix on the SAME host.
 *
 * `derivatives/api/v3/*` carries trading; the ledger and order history live under
 * `api/history/*`. That split is why kraken looked like it had no history axis at all — the
 * routes exist, the client simply could never reach them.
 *
 * It must be a separate client rather than a per-request `baseURL` override:
 * `safeCexAxiosCreate` bakes the private-proxy prefix into the INSTANCE baseURL, so a
 * request-level override discards the proxy and the call egresses from the client's own IP.
 * On an IP-whitelisted key that surfaces as `403 IP is not whitelisted`, which reads as a
 * bad credential.
 *
 * Signing still works out because the interceptor builds `'/api/' + config.url` from the
 * RELATIVE url: `history/v3/account-log` here yields `/api/history/v3/account-log`, matching
 * both the real path and CCXT's `krakenfutures` auth string.
 */
export const HISTORY_BASE_URL = {
  livenet: 'https://futures.kraken.com/api',
  testnet: 'https://demo-futures.kraken.com/api',
};

export type KrakenBaseUrl = 'derivatives' | 'history';

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://futures.kraken.com/ws/v1',
    testnet: 'wss://demo-futures.kraken.com/ws/v1',
  },
  private: {
    livenet: 'wss://futures.kraken.com/ws/v1',
    testnet: 'wss://demo-futures.kraken.com/ws/v1',
  },
  frontendfutures: {
    livenet: 'wss://futures.kraken.com/ws/v1',
    testnet: 'wss://demo-futures.kraken.com/ws/v1',
  },
};

export const ENDPOINTS = {
  /** On the HISTORY base (`historyXhr`), not the derivatives one. */
  ACCOUNT_LOG: '/history/v3/account-log',
  /** On the DERIVATIVES base (`xhr`), unlike the ledger. */
  FILLS: '/v3/fills',
  // Public
  MARKETS: '/v3/instruments',
  TICKERS: '/v3/tickers',
  ORDERBOOK: '/v3/orderbook',
  CANDLES: '/api/charts/v1', // OHLCV endpoint - uses charts API

  // Private (placeholders for future work)
  ACCOUNT: '/v3/accounts',
  OPEN_ORDERS: '/v3/openorders',
  POSITIONS: '/v3/openpositions',
  ORDER: '/v3/sendorder',
  EDIT_ORDER: '/v3/editorder',
  CANCEL_ORDER: '/v3/cancelorder',
  BATCH_ORDERS: '/v3/batchorder',
  LEVERAGE_PREFERENCES: '/v3/leveragepreferences',
};

export const PUBLIC_ENDPOINTS = [ENDPOINTS.MARKETS, ENDPOINTS.TICKERS, ENDPOINTS.ORDERBOOK];

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_MARKET: OrderType.StopLoss,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TRAILING_STOP_MARKET: OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  LONG: PositionSide.Long,
  SHORT: PositionSide.Short,
};

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
  POST_ONLY: OrderTimeInForce.PostOnly,
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  MARK_PRICE: OrderStopTriggerType.MarkPrice,
  LAST_PRICE: OrderStopTriggerType.LastPrice,
  INDEX_PRICE: OrderStopTriggerType.IndexPrice,
};
