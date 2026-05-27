import type { Timeframe } from '../../types';

import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'Tealstreet';

export const BASE_URL = 'https://api.bitget.com';
export const BASE_WS_URL = 'wss://ws.bitget.com/v2/ws/public';

// TODO: these endpoints all use -v2 syntax instead of api/v2/mix/...
export const ENDPOINTS = {
  ACCOUNT_SPOT: '/api/spot/v1/account/getInfo',
  ACCOUNT: '/api/v2/mix/account/account',
  BALANCE: '/api/v2/mix/account/accounts',
  MARKETS: '/api/v2/mix/market/contracts',
  SYMBOL_LEVERAGE: '/api/v2/mix/market/symbol-leverage',
  TICKERS: '/api/v2/mix/market/tickers',
  POSITIONS: '/api/v2/mix/position/all-position',
  POSITIONS_V2: '/api/v2/mix/position/all-position',
  KLINE: '/api/v2/mix/market/candles',
  ORDERS: '/api/v2/mix/order/orders-pending',
  CANCEL_ALGO_ORDER: '/api/v2/mix/order/cancel-plan-order',
  CANCEL_ORDERS: '/api/v2/mix/order/batch-cancel-orders',
  CANCEL_ALL_ORDERS: '/api/v2/mix/order/cancel-all-orders',
  PLACE_ORDER: '/api/v2/mix/order/place-order',
  BATCH_ORDERS: '/api/v2/mix/order/batch-place-order',
  ALGO_ORDERS: '/api/v2/mix/order/orders-plan-pending',
  PLACE_ALGO_ORDER: '/api/v2/mix/order/place-tpsl-order',
  UPDATE_ORDER: '/api/v2/mix/order/modify-order',
  UPDATE_ALGO_ORDER: '/api/v2/mix/order/modify-tpsl-order',
  SET_LEVERAGE: '/api/v2/mix/account/set-leverage',
  SET_LEVERAGE_V3: '/api/v3/account/set-leverage',
  ACCOUNT_SETTINGS_V3: '/api/v3/account/settings',
  POSITION_LEVERAGE: '/api/v2/mix/position/single-position',
  SET_POSITION_MODE: '/api/v2/mix/account/set-position-mode',
  SET_MARGIN_MODE: '/api/v2/mix/account/set-margin-mode',
  PLACE_TRAILING_STOP_ORDER: '/api/v2/mix/plan/place-trail-stop',
  FETCH_SINGLE_ACCOUNT: '/api/v2/mix/account/account',
  ORDER_HISTORY: '/api/v2/mix/order/orders-history',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.SYMBOL_LEVERAGE,
  ENDPOINTS.CANCEL_ALGO_ORDER,
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_ALL_ORDERS,
  ENDPOINTS.PLACE_ORDER,
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.ALGO_ORDERS,
  ENDPOINTS.PLACE_ALGO_ORDER,
  ENDPOINTS.UPDATE_ORDER,
  ENDPOINTS.UPDATE_ALGO_ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.POSITION_LEVERAGE,
  ENDPOINTS.SET_POSITION_MODE,
  ENDPOINTS.SET_MARGIN_MODE,
  ENDPOINTS.PLACE_TRAILING_STOP_ORDER,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.ACCOUNT_SPOT,
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.POSITIONS_V2,
  ENDPOINTS.ORDERS,
  ENDPOINTS.FETCH_SINGLE_ACCOUNT,
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
  '6h': '6Hutc',
  '12h': '12Hutc',
  '1d': '1Dutc',
  '1w': '1Wutc',
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  long: PositionSide.Long,
  short: PositionSide.Short,
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  init: OrderStatus.Open,
  new: OrderStatus.Open,
  not_trigger: OrderStatus.Open,
  partial_fill: OrderStatus.Open,
  partially_fill: OrderStatus.Open,
  partially_filled: OrderStatus.Open,
  triggered: OrderStatus.Closed,
  full_fill: OrderStatus.Closed,
  filled: OrderStatus.Closed,
  fail_trigger: OrderStatus.Canceled,
  cancel: OrderStatus.Canceled,
  cancelled: OrderStatus.Canceled,
  canceled: OrderStatus.Canceled,
  live: OrderStatus.Open,
  fail_execute: OrderStatus.Canceled,
  executed: OrderStatus.Closed,
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
  gtc: OrderTimeInForce.GoodTillCancel,
  post_only: OrderTimeInForce.PostOnly,
  fok: OrderTimeInForce.FillOrKill,
  ioc: OrderTimeInForce.ImmediateOrCancel,
};

// {
//   "code": "00000",
//   "data": {
//   "marginCoin": "USDT",
//     "locked": "0",
//     "available": "13168.86110692",
//     "crossedMaxAvailable": "13168.86110692",
//     "isolatedMaxAvailable": "13168.86110692",
//     "maxTransferOut": "13168.86110692",
//     "accountEquity": "13178.86110692",
//     "usdtEquity": "13178.861106922",
//     "btcEquity": "0.344746495477",
//     "crossedRiskRate": "0",
//     "crossedMarginLeverage": "20",
//     "isolatedLongLever": "20",
//     "isolatedShortLever": "20",
//     "marginMode": "crossed",
//     "posMode": "hedge_mode",
//     "unrealizedPL": "",
//     "coupon": "0"
// },
//   "msg": "success",
//   "requestTime": "1627292199523"
// }

export type FETCH_SINGLE_ACCOUNT_RESPONSE = {
  code: string;
  data: {
    marginCoin: string;
    locked: string;
    available: string;
    crossedMaxAvailable: string;
    isolatedMaxAvailable: string;
    maxTransferOut: string;
    accountEquity: string;
    usdtEquity: string;
    btcEquity: string;
    crossedRiskRate: string;
    crossedMarginLeverage: string;
    isolatedLongLever: string;
    isolatedShortLever: string;
    marginMode: string;
    posMode: string;
    unrealizedPL: string;
    coupon: string;
    assetMode?: string;
  };
  msg: string;
  requestTime: string;
};

// {
//   "code": "00000",
//   "msg": "success",
//   "requestTime": 1714810266272,
//   "data": [
//   {
//     "marginCoin": "USDT",
//     "symbol": "BTCUSDT_UMCBL",
//     "holdSide": "long",
//     "openDelegateCount": "0",
//     "margin": "2.0153312",
//     "available": "0.004",
//     "locked": "0",
//     "total": "0.004",
//     "leverage": 125,
//     "achievedProfits": "0",
//     "averageOpenPrice": "62979.1",
//     "marginMode": "fixed",
//     "holdMode": "double_hold",
//     "unrealizedPL": "0.0084",
//     "liquidationPrice": "62763.981514968857",
//     "keepMarginRate": "0.004",
//     "marketPrice": "62981.2",
//     "marginRatio": "0.572632412843",
//     "autoMargin": "off",
//     "cTime": "1714810218825",
//     "uTime": "1714810218825"
//   }
// ]
// }

export interface FETCH_POSITIONS_V2_RESPONSE {
  code: string;
  msg: string;
  requestTime: number;
  data: Array<{
    marginCoin: string;
    symbol: string;
    holdSide: string;
    openDelegateSize: string;
    marginSize: string;
    available: string;
    locked: string;
    total: string;
    leverage: string;
    achievedProfits: string;
    openPriceAvg: string;
    marginMode: string;
    posMode: string;
    unrealizedPL: string;
    liquidationPrice: string;
    keepMarginRate: string;
    markPrice: string;
    breakEvenPrice: string;
    totalFee: string;
    deductedFee: string;
    takeProfit: string;
    stopLoss: string;
    takeProfitId: string;
    stopLossId: string;
    marginRatio: string;
    assetMode: string;
    cTime: string;
    uTime: string;
  }>;
}
