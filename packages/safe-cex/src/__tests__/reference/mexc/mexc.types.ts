import type { Timeframe } from '../../../types';

import { MarginType, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../../../types';

export const RECV_WINDOW = 20_000;

export const BASE_URL = {
  livenet: 'https://api.mexc.com',
  testnet: 'https://api.mexc.com',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://contract.mexc.com/edge',
    testnet: 'wss://contract.mexc.com/edge',
  },
  private: {
    livenet: 'wss://contract.mexc.com/edge',
    testnet: 'wss://contract.mexc.com/edge',
  },
};

export const MEXC_WS_HEARTBEAT_INTERVAL_MS = 8_000;

export const isMexcControlChannel = (channel: string): boolean => channel.startsWith('rs.');

export const ENDPOINTS = {
  CONTRACT_DETAIL: '/api/v1/contract/detail',
  TICKER: '/api/v1/contract/ticker',
  DEPTH: '/api/v1/contract/depth',
  KLINE: '/api/v1/contract/kline',

  ASSETS: '/api/v1/private/account/assets',
  POSITION_MODE: '/api/v1/private/position/position_mode',
  POSITION_LEVERAGE: '/api/v1/private/position/leverage',
  CHANGE_POSITION_MODE: '/api/v1/private/position/change_position_mode',
  CHANGE_LEVERAGE: '/api/v1/private/position/change_leverage',
  OPEN_POSITIONS: '/api/v1/private/position/open_positions',
  OPEN_ORDERS: '/api/v1/private/order/list/open_orders',
  ORDER_DEALS: '/api/v1/private/order/list/order_deals',
  ORDER_SUBMIT: '/api/v1/private/order/submit',
  ORDER_SUBMIT_BATCH: '/api/v1/private/order/submit_batch',
  ORDER_CANCEL: '/api/v1/private/order/cancel',
  ORDER_CANCEL_ALL: '/api/v1/private/order/cancel_all',
  PLAN_ORDER_LIST: '/api/v1/private/planorder/list/orders',
  PLAN_ORDER_PLACE: '/api/v1/private/planorder/place/v2',
  PLAN_ORDER_CANCEL: '/api/v1/private/planorder/cancel',
  STOP_ORDER_LIST: '/api/v1/private/stoporder/list/orders',
  STOP_ORDER_PLACE: '/api/v1/private/stoporder/place',
  STOP_ORDER_CANCEL: '/api/v1/private/stoporder/cancel',
};

export const PUBLIC_ENDPOINTS = [ENDPOINTS.CONTRACT_DETAIL, ENDPOINTS.TICKER, ENDPOINTS.DEPTH, ENDPOINTS.KLINE];

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER_SUBMIT,
  ENDPOINTS.ORDER_SUBMIT_BATCH,
  ENDPOINTS.PLAN_ORDER_PLACE,
  ENDPOINTS.PLAN_ORDER_CANCEL,
  ENDPOINTS.STOP_ORDER_PLACE,
  ENDPOINTS.STOP_ORDER_CANCEL,
  ENDPOINTS.ORDER_CANCEL,
  ENDPOINTS.ORDER_CANCEL_ALL,
  ENDPOINTS.CHANGE_POSITION_MODE,
  ENDPOINTS.CHANGE_LEVERAGE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ASSETS,
  ENDPOINTS.POSITION_MODE,
  ENDPOINTS.POSITION_LEVERAGE,
  ENDPOINTS.OPEN_POSITIONS,
  ENDPOINTS.OPEN_ORDERS,
];

export const INTERVAL: Partial<Record<Timeframe, string>> = {
  '1m': 'Min1',
  '5m': 'Min5',
  '15m': 'Min15',
  '30m': 'Min30',
  '1h': 'Min60',
  '4h': 'Hour4',
  '1d': 'Day1',
  '1w': 'Week1',
};

export const ORDER_SIDE: Record<number, { side: OrderSide; positionSide: PositionSide; reduceOnly: boolean }> = {
  1: { side: OrderSide.Buy, positionSide: PositionSide.Long, reduceOnly: false },
  2: { side: OrderSide.Buy, positionSide: PositionSide.Short, reduceOnly: true },
  3: { side: OrderSide.Sell, positionSide: PositionSide.Short, reduceOnly: false },
  4: { side: OrderSide.Sell, positionSide: PositionSide.Long, reduceOnly: true },
};

export const REVERSE_ORDER_SIDE: Record<OrderSide, { open: number; close: number }> = {
  [OrderSide.Buy]: { open: 1, close: 2 },
  [OrderSide.Sell]: { open: 3, close: 4 },
};

export const ORDER_TYPE: Record<number, OrderType> = {
  1: OrderType.Limit,
  2: OrderType.Limit,
  3: OrderType.Limit,
  4: OrderType.Limit,
  5: OrderType.Market,
};

export const ORDER_TIME_IN_FORCE: Record<number, OrderTimeInForce> = {
  1: OrderTimeInForce.GoodTillCancel,
  2: OrderTimeInForce.PostOnly,
  3: OrderTimeInForce.ImmediateOrCancel,
  4: OrderTimeInForce.FillOrKill,
  5: OrderTimeInForce.GoodTillCancel,
};

export const REVERSE_ORDER_TYPE: Record<OrderType, number> = {
  [OrderType.Limit]: 1,
  [OrderType.Market]: 5,
  [OrderType.Conditional]: 1,
  [OrderType.StopLoss]: 1,
  [OrderType.TakeProfit]: 1,
  [OrderType.TrailingStopLoss]: 1,
};

export const REVERSE_TIME_IN_FORCE: Record<OrderTimeInForce, number> = {
  [OrderTimeInForce.GoodTillCancel]: 1,
  [OrderTimeInForce.PostOnly]: 2,
  [OrderTimeInForce.ImmediateOrCancel]: 3,
  [OrderTimeInForce.FillOrKill]: 4,
};

export const ORDER_STATUS: Record<number, OrderStatus> = {
  1: OrderStatus.Open,
  2: OrderStatus.Open,
  3: OrderStatus.Closed,
  4: OrderStatus.Canceled,
  5: OrderStatus.Canceled,
};

export const POSITION_SIDE: Record<number, PositionSide> = {
  1: PositionSide.Long,
  2: PositionSide.Short,
};

export const POSITION_MODE: Record<number, PositionMode> = {
  1: PositionMode.Hedged,
  2: PositionMode.OneWay,
};

export const REVERSE_POSITION_MODE: Record<PositionMode, number> = {
  [PositionMode.Hedged]: 1,
  [PositionMode.OneWay]: 2,
};

export const MARGIN_TYPE: Record<number, MarginType> = {
  1: MarginType.Isolated,
  2: MarginType.Cross,
};

export const REVERSE_MARGIN_TYPE: Record<MarginType, number> = {
  [MarginType.Isolated]: 1,
  [MarginType.Cross]: 2,
};

export type MexcEnvelope<T = unknown> = {
  success?: boolean;
  code?: number | string;
  message?: string;
  msg?: string;
  data?: T;
};
