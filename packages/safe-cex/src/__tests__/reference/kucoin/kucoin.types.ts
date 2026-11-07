import type { Timeframe } from '../../../types';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = '3zexp';

export const BASE_URL = 'https://api-futures.kucoin.com';
// We no longer need these hardcoded URLs since we get the endpoint from the token request
// export const BASE_WS_URL = 'wss://ws-api-futures.kucoin.com/endpoint';
// export const BASE_PRIVATE_WS_URL = 'wss://ws-api-futures.kucoin.com/endpoint';

// Correct endpoints according to KuCoin Futures API documentation
export const ENDPOINTS = {
  ACCOUNT_SPOT: '/api/v1/account-overview',
  ACCOUNT: '/api/v1/account-overview',
  BALANCE: '/api/v1/account-overview',
  MARKETS: '/api/v1/contracts/active',
  SYMBOL_LEVERAGE: '/api/v1/position/risk/limit',
  TICKERS: '/api/v1/allTickers',
  ORDERBOOK: '/api/v1/level2/snapshot',
  POSITIONS: '/api/v1/positions',
  POSITIONS_V2: '/api/v1/positions',
  KLINE: '/api/v1/kline/query',
  ORDERS: '/api/v1/orders',
  CANCEL_ALGO_ORDER: '/api/v1/stopOrders/{orderId}',
  CANCEL_ORDERS: '/api/v1/orders',
  CANCEL_ALL_ORDERS: '/api/v3/orders',
  CANCEL_ALL_STOP_ORDERS: '/api/v1/stopOrders',
  CANCEL_SYMBOL_ORDERS: '/api/v3/orders',
  PLACE_ORDER: '/api/v1/orders',
  BATCH_ORDERS: '/api/v1/orders/multi',
  ALGO_ORDERS: '/api/v1/stopOrders',
  PLACE_ALGO_ORDER: '/api/v1/st-orders',
  UPDATE_ORDER: '/api/v1/orders',
  UPDATE_ALGO_ORDER: '/api/v1/stopOrders',
  SET_LEVERAGE: '/api/v2/changeCrossUserLeverage',
  POSITION_LEVERAGE: '/api/v2/getCrossUserLeverage',
  SET_POSITION_MODE: '/api/v2/position/switchPositionMode',
  SET_MARGIN_MODE: '/api/v2/position/changeMarginMode',
  PLACE_TRAILING_STOP_ORDER: '/api/v1/stop-order',
  GET_MARGIN_MODE: '/api/v2/position/getMarginMode',
  GET_POSITION_MODE: '/api/v2/position/getPositionMode',
  FETCH_SINGLE_ACCOUNT: '/api/v2/position',
  // WebSocket endpoints
  GET_PUBLIC_WS_TOKEN: '/api/v1/bullet-public',
  GET_PRIVATE_WS_TOKEN: '/api/v1/bullet-private',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.SYMBOL_LEVERAGE,
  ENDPOINTS.CANCEL_ALGO_ORDER,
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_ALL_ORDERS,
  ENDPOINTS.CANCEL_ALL_STOP_ORDERS,
  ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  ENDPOINTS.PLACE_ORDER,
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.ALGO_ORDERS,
  ENDPOINTS.PLACE_ALGO_ORDER,
  ENDPOINTS.UPDATE_ORDER,
  ENDPOINTS.UPDATE_ALGO_ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.SET_POSITION_MODE,
  ENDPOINTS.SET_MARGIN_MODE,
  ENDPOINTS.PLACE_TRAILING_STOP_ORDER,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.ACCOUNT_SPOT,
  ENDPOINTS.BALANCE,
  ENDPOINTS.ORDERBOOK,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.POSITIONS_V2,
  ENDPOINTS.ORDERS,
  ENDPOINTS.FETCH_SINGLE_ACCOUNT,
  ENDPOINTS.GET_MARGIN_MODE,
  ENDPOINTS.GET_POSITION_MODE,
  ENDPOINTS.POSITION_LEVERAGE,
];

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
  '1d': '1440',
  '1w': '10080',
};

export const WS_INTERVAL: Record<Timeframe, string> = {
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

export const POSITION_SIDE: Record<string, PositionSide> = {
  long: PositionSide.Long,
  short: PositionSide.Short,
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  init: OrderStatus.Open,
  new: OrderStatus.Open,
  open: OrderStatus.Open,
  match: OrderStatus.Open,
  not_trigger: OrderStatus.Open,
  partially_filled: OrderStatus.Open,
  filled: OrderStatus.Closed,
  canceled: OrderStatus.Canceled,
  cancelled: OrderStatus.Canceled,
  done: OrderStatus.Closed,
};

export const ORDER_TYPE: Record<string, OrderType> = {
  limit: OrderType.Limit,
  market: OrderType.Market,
  pos_profit: OrderType.TakeProfit,
  pos_loss: OrderType.StopLoss,
  loss_plan: OrderType.StopLoss,
  profit_plan: OrderType.TakeProfit,
  track_plan: OrderType.TrailingStopLoss,
  tp: OrderType.TakeProfit,
  sl: OrderType.StopLoss,
  psl: OrderType.StopLoss,
  ptp: OrderType.TakeProfit,
  track: OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  open_long: OrderSide.Buy,
  open_short: OrderSide.Sell,
  close_long: OrderSide.Sell,
  close_short: OrderSide.Buy,
  reduce_close_long: OrderSide.Sell,
  reduce_close_short: OrderSide.Buy,
  offset_close_long: OrderSide.Sell,
  offset_close_short: OrderSide.Buy,
  burst_close_long: OrderSide.Sell,
  burst_close_short: OrderSide.Buy,
  delivery_close_long: OrderSide.Sell,
  delivery_close_short: OrderSide.Buy,
  buy_single: OrderSide.Buy,
  sell_single: OrderSide.Sell,
  buy: OrderSide.Buy,
  sell: OrderSide.Sell,
};

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  normal: OrderTimeInForce.GoodTillCancel,
  GTC: OrderTimeInForce.GoodTillCancel,
  post_only: OrderTimeInForce.PostOnly,
  fok: OrderTimeInForce.FillOrKill,
  ioc: OrderTimeInForce.ImmediateOrCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
};
