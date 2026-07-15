import type { Timeframe } from '../../types';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'b3cdedc0a20880ba'; // tealstreet
export const USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY = false;

export const BASE_URL = {
  livenet: 'https://openapi.blofin.com',
  testnet: 'https://demo-trading-openapi.blofin.com',
};
export const BASE_WS_URL = {
  public: {
    livenet: 'wss://openapi.blofin.com/ws/public',
    testnet: 'wss://demo-trading-openapi.blofin.com/ws/public',
  },
  private: {
    livenet: 'wss://openapi.blofin.com/ws/private',
    testnet: 'wss://demo-trading-openapi.blofin.com/ws/private',
  },
};

export const ENDPOINTS = {
  MARKETS: '/api/v1/market/instruments',
  TICKERS: '/api/v1/market/tickers',
  BALANCE: '/api/v1/asset/balances',
  POSITIONS: '/api/v1/account/positions',
  KLINE: '/api/v1/market/candles',
  FILLS: '/api/v1/trade/fills-history',
  ORDERS_HISTORY: '/api/v1/trade/orders-history',
  UNFILLED_ORDERS: '/api/v1/trade/orders-pending',
  UNFILLED_ALGO_ORDERS: '/api/v1/trade/orders-tpsl-pending',
  UNFILLED_TRIGGER_ORDERS: '/api/v1/trade/orders-algo-pending',
  CANCEL_ORDERS: '/api/v1/trade/cancel-batch-orders',
  CANCEL_ALGO_ORDER: '/api/v1/trade/cancel-tpsl',
  PLACE_ORDERS: '/api/v1/trade/order',
  PLACE_ORDERS_BATCH: '/api/v1/trade/batch-orders',
  PLACE_TPSL_ORDER: '/api/v1/trade/order-tpsl',
  SET_LEVERAGE: '/api/v1/account/set-leverage',
  LEVERAGE: '/api/v1/account/batch-leverage-info',
  MARGIN_MODE: '/api/v1/account/margin-mode',
  POSITION_MODE: '/api/v1/account/position-mode',
  SET_MARGIN_MODE: '/api/v1/account/set-margin-mode',
  SET_POSITION_MODE: '/api/v1/account/set-position-mode',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_ALGO_ORDER,
  ENDPOINTS.PLACE_ORDERS,
  ENDPOINTS.PLACE_ORDERS_BATCH,
  ENDPOINTS.PLACE_TPSL_ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.LEVERAGE,
  ENDPOINTS.MARGIN_MODE,
  ENDPOINTS.POSITION_MODE,
  ENDPOINTS.SET_MARGIN_MODE,
  ENDPOINTS.SET_POSITION_MODE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.UNFILLED_ORDERS,
  ENDPOINTS.UNFILLED_ALGO_ORDERS,
  ENDPOINTS.UNFILLED_TRIGGER_ORDERS,
];

export const INTERVAL: Record<Timeframe, string> = {
  '1m': '1m',
  '3m': '3m',
  '5m': '5m',
  '15m': '15m',
  '30m': '30m',
  '1h': '1H',
  '2h': '2H',
  '4h': '4H',
  '6h': '6H',
  '12h': '12H',
  '1d': '1D',
  '1w': '1W',
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  live: OrderStatus.Open,
  partially_filled: OrderStatus.Open,
};

export const ORDER_TYPE: Record<string, OrderType> = {
  market: OrderType.Market,
  limit: OrderType.Limit,
  post_only: OrderType.Limit,
  fok: OrderType.Limit,
  ioc: OrderType.Limit,
  optimal_limit_ioc: OrderType.Market,
};

export const REVERSE_ORDER_TYPE: Record<OrderType, string> = {
  [OrderType.Market]: 'market',
  [OrderType.Limit]: 'limit',
  [OrderType.Conditional]: 'trigger',
  [OrderType.StopLoss]: 'trigger',
  [OrderType.TakeProfit]: 'trigger',
  [OrderType.TrailingStopLoss]: 'move_order_stop',
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  buy: OrderSide.Buy,
  sell: OrderSide.Sell,
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  long: PositionSide.Long,
  short: PositionSide.Short,
};

// {
//   "positionId": "2257167",
//   "instId": "CKB-USDT",
//   "instType": "SWAP",
//   "marginMode": "isolated",
//   "positionSide": "long",
//   "adl": "2",
//   "positions": "10",
//   "availablePositions": "10",
//   "averagePrice": "0.020246000000000000",
//   "margin": "0.067486666666666666",
//   "markPrice": "0.017627495167",
//   "marginRatio": "11.373895780256895883",
//   "liquidationPrice": "0.013781226601320536",
//   "unrealizedPnl": "-0.02618504833",
//   "unrealizedPnlRatio": "-0.388003284550034578",
//   "maintenanceMargin": "0.0035254990334",
//   "createTime": "1711116782699",
//   "updateTime": "1711116785659",
//   "leverage": "3"
// }
export type FETCH_POSITION_RESPONSE = {
  code: string;
  msg: string;
  data: {
    positionId: string;
    instId: string;
    instType: string;
    marginMode: string;
    positionSide: string;
    adl: string;
    positions: string;
    availablePositions: string;
    averagePrice: string;
    margin: string;
    markPrice: string;
    marginRatio: string;
    liquidationPrice: string;
    unrealizedPnl: string;
    unrealizedPnlRatio: string;
    maintenanceMargin: string;
    createTime: string;
    updateTime: string;
    leverage: string;
  }[];
};

// {
//   "code": "0",
//   "msg": "success",
//   "data": {
//   "positionMode": "net_mode"
// }
// }

export type POSITION_MODE_RESPONSE = {
  code: string;
  msg: string;
  data: {
    positionMode: string;
  };
};

// {
//   "code": "0",
//   "msg": "success",
//   "data": {
//   "marginMode": "isolated"
// }
// }
export type FETCH_MARGIN_MODE_RESPONSE = {
  code: string;
  msg: string;
  data: {
    marginMode: string;
  };
};

// {
//   "code": "0",
//   "msg": "success",
//   "data": [
//   {
//     "leverage": "50",
//     "marginMode": "cross",
//     "instId": "BTC-USDT",
//     "positionSide":"net"
//   },
//   {
//     "leverage": "3",
//     "marginMode": "cross",
//     "instId": "ETH-USDT",
//     "positionSide":"net"
//   }
// ]
// }
export type FETCH_BATCH_LEV_RESPONSE = {
  code: string;
  msg: string;
  data: {
    leverage: string;
    marginMode: string;
    instId: string;
    positionSide: 'net' | 'long' | 'short';
  }[];
};

// {
//   "code": "152401",
//   "msg": "Access key does not exist"
// }
export type ORDERS_RESPONSE = {
  code: string;
  msg?: string;
  data?: {
    orderId: string;
    instId: string;
    tdMode: string;
    cTime: string;
    uTime: string;
    price: string;
    size: string;
    side: string;
    orderType: string;
    state: string;
    filledSize: string;
    avgFillPrice: string;
    fee: string;
    feeCurrency: string;
  }[];
};

//
// {
//   "code": "0",
//   "msg": "success",
//   "data": [
//   {
//     "positionId": "395624",
//     "instId": "BTC-USDT",
//     "instType": "SWAP",
//     "marginMode": "cross",
//     "positionSide": "net",
//     "adl": "2",
//     "positions": "4",
//     "availablePositions": "4",
//     "averagePrice": "59965.800000000000000000",
//     "markPrice": "60093.894633",
//     "marginRatio": "152.753225921391424397",
//     "liquidationPrice": "26914.958140225036518121",
//     "unrealizedPnl": "0.512378532",
//     "unrealizedPnlRatio": "0.15166509815594889",
//     "initialMargin": "3.385571528619718309",
//     "maintenanceMargin": "0.721126735596",
//     "createTime": "1719283124435",
//     "updateTime": "1723787570821",
//     "leverage": "71"
//   },
//   {
//     "positionId": "4143067",
//     "instId": "ACH-USDT",
//     "instType": "SWAP",
//     "marginMode": "cross",
//     "positionSide": "net",
//     "adl": "4",
//     "positions": "2",
//     "availablePositions": "2",
//     "averagePrice": "0.018424000000000000",
//     "markPrice": "0.019388747967000002",
//     "marginRatio": "152.753225921391424397",
//     "liquidationPrice": "",
//     "unrealizedPnl": "0.01929495934000004",
//     "unrealizedPnlRatio": "0.157090962928788863",
//     "initialMargin": "0.129258319780000013",
//     "maintenanceMargin": "0.0058166243901000006",
//     "createTime": "1723699419439",
//     "updateTime": "1723699419445",
//     "leverage": "3"
//   }
// ]
// }
export type POSITIONS_RESPONSE = {
  code: string;
  msg: string;
  data?: {
    positionId: string;
    instId: string;
    instType: string;
    marginMode: string;
    positionSide: string;
    adl: string;
    positions: string;
    availablePositions: string;
    averagePrice: string;
    markPrice: string;
    marginRatio: string;
    liquidationPrice: string;
    unrealizedPnl: string;
    unrealizedPnlRatio: string;
    initialMargin: string;
    maintenanceMargin: string;
    createTime: string;
    updateTime: string;
    leverage: string;
  }[];
};
