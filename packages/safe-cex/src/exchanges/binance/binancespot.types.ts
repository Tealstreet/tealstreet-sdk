import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://api.binance.com',
  testnet: 'https://testnet.binance.vision',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://stream.binance.com:9443/ws',
    testnet: 'wss://testnet.binance.vision/ws',
  },
  private: {
    livenet: 'wss://ws-api.binance.com:443/ws-api/v3',
    testnet: 'wss://ws-api.testnet.binance.vision/ws-api/v3',
  },
};

// Binance removed the Spot listen-key REST endpoints on 2026-02-20. Private
// events now use a signed WebSocket API subscription. Keep this window small,
// matching Binance's recommendation for signed WebSocket API requests.
export const USER_DATA_RECV_WINDOW = 5_000;

export const ENDPOINTS = {
  ACCOUNT: '/api/v3/account',
  MARKETS: '/api/v3/exchangeInfo',
  TICKERS_24H: '/api/v3/ticker/24hr',
  ORDER: '/api/v3/order',
  CANCEL_SYMBOL_ORDERS: '/api/v3/openOrders',
  OPEN_ORDERS: '/api/v3/openOrders',
  KLINE: '/api/v3/klines',
  ORDERBOOK: '/api/v3/depth',
};

export const PUBLIC_ENDPOINTS = [ENDPOINTS.MARKETS, ENDPOINTS.TICKERS_24H, ENDPOINTS.KLINE, ENDPOINTS.ORDERBOOK];

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.ORDER, ENDPOINTS.CANCEL_SYMBOL_ORDERS];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ACCOUNT, ENDPOINTS.OPEN_ORDERS];

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_MARKET: OrderType.StopLoss,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TRAILING_STOP_MARKET: OrderType.TrailingStopLoss,
};

// Spot `executionReport` reports the order type in the `o` field with SPOT
// values (LIMIT/MARKET/STOP_LOSS[_LIMIT]/TAKE_PROFIT[_LIMIT]/LIMIT_MAKER) —
// NOT the futures `ot` field or the futures values (STOP_MARKET, etc.).
// LIMIT_MAKER is a post-only limit → OrderType.Limit. Spot has no trailing
// stop. See https://developers.binance.com/docs/binance-spot-api-docs/user-data-stream
export const ORDER_TYPE_SPOT: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  LIMIT_MAKER: OrderType.Limit,
  STOP_LOSS: OrderType.StopLoss,
  STOP_LOSS_LIMIT: OrderType.StopLoss,
  TAKE_PROFIT: OrderType.TakeProfit,
  TAKE_PROFIT_LIMIT: OrderType.TakeProfit,
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
  GTX: OrderTimeInForce.PostOnly,
};
