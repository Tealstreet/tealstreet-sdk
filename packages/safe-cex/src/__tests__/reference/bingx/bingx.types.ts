import { OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://open-api.bingx.com',
  testnet: 'https://open-api.bingx.com',
};

export const BASE_WS_URL = {
  public: {
    // livenet: 'wss://open-api-swap.bingx.com/swap-market',
    // testnet: 'wss://open-api-swap.bingx.com/swap-market',
    livenet: 'wss://ws-market-swap.qq-os.com/ws',
    testnet: 'wss://ws-market-swap.qq-os.com/ws',
  },
  private: {
    livenet: 'wss://open-api-swap.bingx.com/swap-market',
    testnet: 'wss://open-api-swap.bingx.com/swap-market',
  },
};

export const ENDPOINTS = {
  BALANCE: '/openApi/swap/v3/user/balance',
  MARKETS: '/openApi/swap/v2/quote/contracts',
  POSITIONS: '/openApi/swap/v2/user/positions',
  LEVERAGE: '/openApi/swap/v2/trade/leverage',
  TICKERS_24H: '/openApi/swap/v2/quote/ticker',
  TICKERS_PRICE: '/openApi/swap/v2/quote/premiumIndex',
  HEDGE_MODE: '/openApi/swap/v1/positionSide/dual',
  MARGIN_MODE: '/openApi/swap/v2/trade/marginType',
  SET_LEVERAGE: '/openApi/swap/v2/trade/leverage',
  OPEN_ORDERS: '/openApi/swap/v2/trade/openOrders',
  CANCEL_SYMBOL_ORDERS: '/openApi/swap/v2/trade/allOpenOrders',
  ORDER: '/openApi/swap/v2/trade/order',
  AMEND_ORDER: '/openApi/swap/v1/trade/amend',
  CANCEL_REPLACE_ORDER: '/openApi/swap/v1/trade/cancelReplace',
  BATCH_ORDERS: '/openApi/swap/v2/trade/batchOrders',
  KLINE: '/openApi/swap/v3/quote/klines',
  LISTEN_KEY: '/openApi/user/auth/userDataStream',
  ORDERBOOK: '/openApi/swap/v2/quote/depth',
  USER_TRADES: '/openApi/swap/v2/trade/fillHistory',
};

export type BINGX_BALANCE = {
  userId: string;
  asset: string;
  balance: string;
  equity: string;
  unrealizedProfit: string;
  realisedProfit: string;
  availableMargin: string;
  usedMargin: string;
  freezedMargin: string;
};

export type FETCH_BALANCE_RESPONSE = {
  code: number;
  msg: string;
  data: BINGX_BALANCE[];
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER,
  ENDPOINTS.AMEND_ORDER,
  ENDPOINTS.CANCEL_REPLACE_ORDER,
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  ENDPOINTS.HEDGE_MODE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.OPEN_ORDERS,
  ENDPOINTS.LEVERAGE,
];

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  // needs to be before STOP_MARKET so inverseObj overwrites it as we want STOP_MARKET as default
  TRIGGER_MARKET: OrderType.StopLoss,
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
  PostOnly: OrderTimeInForce.PostOnly,
};

export type BINGX_ORDER_STATUS =
  | 'CANCELED'
  | 'FILLED'
  | 'NEW'
  | 'OPEN'
  | 'PARTIALLY_FILLED'
  | 'PENDING'
  | 'WORKING'
  | 'EXPIRED';
export const ORDER_STATUS: Record<BINGX_ORDER_STATUS, OrderStatus> = {
  CANCELED: OrderStatus.Canceled,
  FILLED: OrderStatus.Closed,
  NEW: OrderStatus.Open,
  OPEN: OrderStatus.Open,
  PARTIALLY_FILLED: OrderStatus.Open,
  PENDING: OrderStatus.Open,
  WORKING: OrderStatus.Open,
  EXPIRED: OrderStatus.Closed,
};

// {
//   "code": 0,
//   "msg": "",
//   "data": {
//   "longLeverage": 80,
//     "shortLeverage": 100,
//     "maxLongLeverage": 125,
//     "maxShortLeverage": 125,
//     "availableLongVol": "0.3275",
//     "availableShortVol": "0.4055",
//     "availableLongVal": "18804.4",
//     "availableShortVal": "23281.7",
//     "maxPositionLongVal": "900000.0",
//     "maxPositionShortVal": "900000.0"
// }
// }
export type FETCH_LEVERAGE_RESPONSE = {
  code: number;
  msg: string;
  data: {
    longLeverage: number;
    shortLeverage: number;
    maxLongLeverage: number;
    maxShortLeverage: number;
    availableLongVol: string;
    availableShortVol: string;
    availableLongVal: string;
    availableShortVal: string;
    maxPositionLongVal: string;
    maxPositionShortVal: string;
  };
};

// {
//   "code": 0,
//   "msg": "",
//   "data": {
//   "marginType": "CROSSED"
// }
// }

export type FETCH_MARGIN_MODE_RESPONSE = {
  code: number;
  msg: string;
  data: {
    marginType: 'CROSSED' | 'ISOLATED';
  };
};

// {
//   "code": 0,
//   "msg": "",
//   "timeStamp": 1714575698943,
//   "data": {
//   "dualSidePosition": "true"
// }

export type FETCH_HEDGE_MODE_RESPONSE = {
  code: number;
  msg: string;
  timeStamp: number;
  data: {
    dualSidePosition: 'true' | 'false';
  };
};

// {
//   "e": "ORDER_TRADE_UPDATE",
//   "E": 1714577902984,
//   "o": {
//   "s": "BTC-USDT",
//     "c": "",
//     "i": "1785695267203530800",
//     "S": "BUY",
//     "o": "LIMIT",
//     "q": "0.00030000",
//     "p": "42967.20000000",
//     "sp": "0.00000000",
//     "ap": "0.00000000",
//     "x": "TRADE",
//     "X": "NEW",
//     "N": "USDT",
//     "n": "0.00000000",
//     "T": 0,
//     "wt": "MARK_PRICE",
//     "ps": "BOTH",
//     "rp": "0.00000000",
//     "z": "0.00000000",
//     "sg": "false",
//     "ti": 0
// }
// }

export type ORDER_TRADE_UPDATE = {
  e: 'ORDER_TRADE_UPDATE';
  E: number;
  o: {
    s: string;
    // c: string;
    i: string;
    S: string;
    o: string;
    q: string;
    p: string;
    sp: string;
    ap: string;
    x: string;
    X: BINGX_ORDER_STATUS;
    N: string;
    n: string;
    T: number;
    wt: string;
    ps: string;
    rp: string;
    z: string;
    sg: string;
    ti: number;
    R?: boolean;
  };
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  MARK_PRICE: OrderStopTriggerType.MarkPrice,
  CONTRACT_PRICE: OrderStopTriggerType.LastPrice,
};
