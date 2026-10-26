import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://fapi.bitunix.com',
  testnet: 'https://testnet.bitunix.com',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://fapi.bitunix.com/public/',
    testnet: 'wss://fapi.bitunix.com/public/',
  },
  private: {
    livenet: 'wss://fapi.bitunix.com/private/',
    testnet: 'wss://fapi.bitunix.com/private/',
  },
  frontendfutures: {
    livenet: 'wss://api.bitunix.com/ws-futures/?from=contract&compress=2',
    testnet: 'wss://api.bitunix.com/ws-futures/?from=contract&compress=2',
  },
};

export const ENDPOINTS = {
  MARKETS: '/api/v1/futures/market/trading_pairs',
  ACCOUNT: '/api/v1/futures/account',
  TICKERS: '/api/v1/futures/market/tickers',
  LEVERAGE_MARGIN_MODE: '/api/v1/futures/account/get_leverage_margin_mode',
  HEDGE_MODE: '/api/v1/futures/account/change_position_mode',
  SET_LEVERAGE: '/api/v1/futures/account/change_leverage',
  OPEN_ORDERS: '/api/v1/futures/trade/get_pending_orders',
  OPEN_TPSL_ORDERS: '/api/v1/futures/tpsl/get_pending_orders',
  ORDER: '/api/v1/futures/trade/place_order',
  MODIFY_ORDER: '/api/v1/futures/trade/modify_order',
  TP_SL_ORDER: '/api/v1/futures/tpsl/place_order',
  POSITION_TP_SL_ORDER: '/api/v1/futures/tpsl/position/place_order',
  MODIFY_TP_SL_ORDER: '/api/v1/futures/tpsl/modify_order',
  MODIFY_POSITION_TP_SL_ORDER: '/api/v1/futures/tpsl/position/modify_order',
  BATCH_ORDERS: '/api/v1/futures/trade/batch_order',
  CANCEL_ORDER_TP_SL: '/api/v1/futures/tpsl/cancel_order',
  CANCEL_ORDER: '/api/v1/futures/trade/cancel_orders',
  KLINE: '/api/v1/futures/market/kline',
  HISTORY_TRADES: '/api/v1/futures/trade/get_history_trades',
  // Order history, including orders that never filled. Rows nest under `data.orderList`.
  HISTORY_ORDERS: '/api/v1/futures/trade/get_history_orders',
  // Closed positions. A DIFFERENT source from HISTORY_TRADES, which is what lets it
  // cross-check the fills-derived ledger. `/api/v1/futures/account/get_bills` is 404.
  HISTORY_POSITIONS: '/api/v1/futures/position/get_history_positions',
  SET_MARGIN_TYPE: '/api/v1/futures/account/change_margin_mode',
  POSITIONS: '/api/v1/futures/position/get_pending_positions',
};

export const PUBLIC_ENDPOINTS = [ENDPOINTS.MARKETS, ENDPOINTS.TICKERS, ENDPOINTS.KLINE];

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER,
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.MODIFY_TP_SL_ORDER,
  ENDPOINTS.MODIFY_POSITION_TP_SL_ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.HEDGE_MODE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ACCOUNT, ENDPOINTS.OPEN_ORDERS, ENDPOINTS.POSITIONS];

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
  MARK: OrderStopTriggerType.MarkPrice,
  MARK_PRICE: OrderStopTriggerType.MarkPrice,
  LAST: OrderStopTriggerType.LastPrice,
  LAST_PRICE: OrderStopTriggerType.LastPrice,
  // WS uses short names while REST uses UPPER_SNAKE
  Mark: OrderStopTriggerType.MarkPrice,
  Last: OrderStopTriggerType.LastPrice,
};

// [
//   {
//     "entryPrice": "0.00000",
//     "breakEvenPrice": "0.0",
//     "marginType": "isolated",
//     "isAutoAddMargin": "false",
//     "isolatedMargin": "0.00000000",
//     "leverage": "10",
//     "liquidationPrice": "0",
//     "markPrice": "6679.50671178",
//     "maxNotionalValue": "20000000",
//     "positionAmt": "0.000",
//     "notional": "0",,
//   "isolatedWallet": "0",
//   "symbol": "BTCUSDT",
//   "unRealizedProfit": "0.00000000",
//   "positionSide": "BOTH",
//   "updateTime": 0
// }
// ]

// {
//   "symbol": "CTSIUSDT",
//   "positionAmt": "0",
//   "entryPrice": "0.0",
//   "breakEvenPrice": "0.0",
//   "markPrice": "0.00000000",
//   "unRealizedProfit": "0.00000000",
//   "liquidationPrice": "0",
//   "leverage": "20",
//   "maxNotionalValue": "25000",
//   "marginType": "cross",
//   "isolatedMargin": "0.00000000",
//   "isAutoAddMargin": "false",
//   "positionSide": "SHORT",
//   "notional": "0",
//   "isolatedWallet": "0",
//   "updateTime": 0,
//   "isolated": false,
//   "adlQuantile": 0
// }
export type GET_PENDING_POSITIONS_RESPONSE = {
  symbol: string;
  positionId: string;
  qty: string;
  maxQty?: string;
  entryValue: string;
  entryPrice?: string;
  avgOpenPrice?: string;
  side: 'BUY' | 'SELL' | 'LONG' | 'SHORT';
  marginMode: 'ISOLATION' | 'CROSS';
  positionMode: 'ONE_WAY' | 'HEDGED' | 'HEDGE';
  leverage: number;
  fee: string;
  funding: string;
  realizedPNL: string;
  margin: string;
  unrealizedPNL: string;
  liqPrice: string;
  marginRate?: string;
  ctime: number;
  mtime: number;
}[];

export type KlineInterval =
  | '1min'
  | 'mark_kline_1min'
  | '3min'
  | 'mark_kline_3min'
  | '5min'
  | 'mark_kline_5min'
  | '15min'
  | 'mark_kline_15min'
  | '30min'
  | 'mark_kline_30min'
  | '60min'
  | 'mark_kline_60min'
  | '2h'
  | 'mark_kline_2h'
  | '4h'
  | 'mark_kline_4h'
  | '6h'
  | 'mark_kline_6h'
  | '8h'
  | 'mark_kline_8h'
  | '12h'
  | 'mark_kline_12h'
  | '1day'
  | 'mark_kline_1day'
  | '3day'
  | 'mark_kline_3day'
  | '1week'
  | 'mark_kline_1week'
  | '1month'
  | 'mark_kline_1month';
