import type { Timeframe } from '../../../types';

import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../../types';

export const RECV_WINDOW = 20_000;

export const BASE_URL = {
  livenet: 'https://api-contract.weex.com',
  testnet: 'https://api-contract.weex.com',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://ws-contract.weex.com/v3/ws/public',
    testnet: 'wss://ws-contract.weex.com/v3/ws/public',
  },
  private: {
    livenet: 'wss://ws-contract.weex.com/v3/ws/private',
    testnet: 'wss://ws-contract.weex.com/v3/ws/private',
  },
};

export const PRIVATE_WS_REQUEST_PATH = '/v3/ws/private';

export const ENDPOINTS = {
  EXCHANGE_INFO: '/capi/v3/market/exchangeInfo',
  TICKER_24HR: '/capi/v3/market/ticker/24hr',
  BOOK_TICKER: '/capi/v3/market/ticker/bookTicker',
  DEPTH: '/capi/v3/market/depth',
  KLINES: '/capi/v3/market/klines',
  HISTORY_KLINES: '/capi/v3/market/historyKlines',
  PUBLIC_TRADES: '/capi/v3/market/trades',

  ACCOUNT_CONFIG: '/capi/v3/account/accountConfig',
  BALANCE: '/capi/v3/account/balance',
  SYMBOL_CONFIG: '/capi/v3/account/symbolConfig',
  INCOME: '/capi/v3/account/income',
  POSITIONS: '/capi/v3/account/position/allPosition',
  SINGLE_POSITION: '/capi/v3/account/position/singlePosition',
  SET_MARGIN_TYPE: '/capi/v3/account/marginType',
  SET_LEVERAGE: '/capi/v3/account/leverage',

  ORDER: '/capi/v3/order',
  BATCH_ORDERS: '/capi/v3/batchOrders',
  OPEN_ORDERS: '/capi/v3/openOrders',
  CANCEL_ALL_OPEN_ORDERS: '/capi/v3/allOpenOrders',
  ORDER_HISTORY: '/capi/v3/order/history',
  USER_TRADES: '/capi/v3/userTrades',
  ALGO_ORDER: '/capi/v3/algoOrder',
  OPEN_ALGO_ORDERS: '/capi/v3/openAlgoOrders',
  CANCEL_ALL_ALGO_ORDERS: '/capi/v3/algoOpenOrders',
  PLACE_TP_SL_ORDER: '/capi/v3/placeTpSlOrder',
  MODIFY_TP_SL_ORDER: '/capi/v3/modifyTpSlOrder',
  CLOSE_POSITIONS: '/capi/v3/closePositions',
};

export const PUBLIC_ENDPOINTS = [
  ENDPOINTS.EXCHANGE_INFO,
  ENDPOINTS.TICKER_24HR,
  ENDPOINTS.BOOK_TICKER,
  ENDPOINTS.DEPTH,
  ENDPOINTS.KLINES,
  ENDPOINTS.HISTORY_KLINES,
  ENDPOINTS.PUBLIC_TRADES,
];

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER,
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.SET_MARGIN_TYPE,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.ALGO_ORDER,
  ENDPOINTS.PLACE_TP_SL_ORDER,
  ENDPOINTS.MODIFY_TP_SL_ORDER,
  ENDPOINTS.CLOSE_POSITIONS,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNT_CONFIG,
  ENDPOINTS.BALANCE,
  ENDPOINTS.SYMBOL_CONFIG,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.OPEN_ORDERS,
  ENDPOINTS.OPEN_ALGO_ORDERS,
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

export const SUPPORTED_INTERVALS = new Set(['1m', '5m', '15m', '30m', '1h', '4h', '12h', '1d', '1w']);
export const SUPPORTED_WS_INTERVALS = new Set([...SUPPORTED_INTERVALS, '2h', '6h']);

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const REVERSE_ORDER_SIDE: Record<OrderSide, string> = {
  [OrderSide.Buy]: 'BUY',
  [OrderSide.Sell]: 'SELL',
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  LONG: PositionSide.Long,
  SHORT: PositionSide.Short,
};

export const REVERSE_POSITION_SIDE: Record<PositionSide, string> = {
  [PositionSide.Long]: 'LONG',
  [PositionSide.Short]: 'SHORT',
};

export const ORDER_TYPE: Record<string, OrderType> = {
  CONDITIONAL: OrderType.Conditional,
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP: OrderType.StopLoss,
  STOP_MARKET: OrderType.StopLoss,
  STOP_LOSS: OrderType.StopLoss,
  TAKE_PROFIT: OrderType.TakeProfit,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TRAILING_STOP_MARKET: OrderType.TrailingStopLoss,
};

export const REVERSE_ORDER_TYPE: Record<OrderType, string> = {
  [OrderType.Limit]: 'LIMIT',
  [OrderType.Market]: 'MARKET',
  [OrderType.Conditional]: 'STOP_MARKET',
  [OrderType.StopLoss]: 'STOP_MARKET',
  [OrderType.TakeProfit]: 'TAKE_PROFIT_MARKET',
  [OrderType.TrailingStopLoss]: 'TRAILING_STOP_MARKET',
};

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
  POST_ONLY: OrderTimeInForce.PostOnly,
};

export const REVERSE_TIME_IN_FORCE: Record<OrderTimeInForce, string> = {
  [OrderTimeInForce.GoodTillCancel]: 'GTC',
  [OrderTimeInForce.ImmediateOrCancel]: 'IOC',
  [OrderTimeInForce.FillOrKill]: 'FOK',
  [OrderTimeInForce.PostOnly]: 'POST_ONLY',
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  NEW: OrderStatus.Open,
  PENDING: OrderStatus.Open,
  PARTIALLY_FILLED: OrderStatus.Open,
  UNTRIGGERED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELED: OrderStatus.Canceled,
  CANCELLED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Canceled,
  REJECTED: OrderStatus.Canceled,
  CANCELING: OrderStatus.Canceling,
};

export const MARGIN_TYPE: Record<string, MarginType> = {
  CROSSED: MarginType.Cross,
  CROSS: MarginType.Cross,
  SHARED: MarginType.Cross,
  ISOLATED: MarginType.Isolated,
  ISOLATION: MarginType.Isolated,
};

export const REVERSE_MARGIN_TYPE: Record<MarginType, string> = {
  [MarginType.Cross]: 'CROSSED',
  [MarginType.Isolated]: 'ISOLATED',
};

export const POSITION_MODE: Record<string, PositionMode> = {
  COMBINED: PositionMode.OneWay,
  SEPARATED: PositionMode.Hedged,
};

export const REVERSE_POSITION_MODE: Record<PositionMode, string> = {
  [PositionMode.OneWay]: 'COMBINED',
  [PositionMode.Hedged]: 'SEPARATED',
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  CONTRACT_PRICE: OrderStopTriggerType.LastPrice,
  LAST_PRICE: OrderStopTriggerType.LastPrice,
  MARK_PRICE: OrderStopTriggerType.MarkPrice,
};

export const REVERSE_ORDER_TRIGGER_TYPE: Record<OrderStopTriggerType, string> = {
  [OrderStopTriggerType.LastPrice]: 'CONTRACT_PRICE',
  [OrderStopTriggerType.MarkPrice]: 'MARK_PRICE',
  [OrderStopTriggerType.IndexPrice]: 'MARK_PRICE',
};

export type WeexKlineRow = [
  number | string,
  string,
  string,
  string,
  string,
  string,
  number | string,
  string,
  number | string,
  string,
  string,
];

export type WeexExchangeInfoResponse = {
  assets?: Array<Record<string, any>>;
  symbols?: Array<Record<string, any>>;
};

export type WeexOrderResult = {
  orderId?: string;
  clientOrderId?: string;
  success?: boolean;
  errorCode?: string;
  errorMessage?: string;
};
