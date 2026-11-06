import { OrderSide, OrderStatus, OrderType, Timeframe } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'Tealstreet';

export const BASE_URL = 'https://fapi.bitrue.com';

export const BASE_WS_URL = {
  // Bitrue's documented USDT-M futures market-data stream. Do not add the
  // similarly named spot host here: its futures channel subscriptions go dark.
  public: 'wss://fmarket-ws.bitrue.com/kline-api/ws',
  private: 'wss://fapiws.bitrue.com/stream',
};

export const ENDPOINTS = {
  ACCOUNT: '/fapi/v2/account',
  KLINE: '/fapi/v1/klines',
  MARKETS: '/fapi/v1/contracts',
  TICKERS: '/fapi/v1/ticker',
  LEVERAGE_BRACKET: '/fapi/v2/leverageBracket',
  ORDERS: '/fapi/v2/openOrders',
  MY_TRADES: '/fapi/v2/myTrades',
  PLACE_ORDER: '/fapi/v2/order',
  CANCEL_ORDER: '/fapi/v2/cancel',
  LISTEN_KEY: 'https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey',
  SET_LEVERAGE: '/fapi/v2/level_edit',
};

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.PLACE_ORDER];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ORDERS, ENDPOINTS.ACCOUNT];

export const INTERVAL: { [key in Timeframe]?: string } = {
  '1m': '1min',
  '5m': '5min',
  '15m': '15min',
  '30m': '30min',
  '1h': '60min',
  '1d': '1day',
  '1w': '1week',
};

export type BITRUE_ORDER_TYPE = 'LIMIT' | 'MARKET' | 'STOP_MARKET' | 'TAKE_PROFIT_MARKET';
export const ORDER_TYPE: Record<BITRUE_ORDER_TYPE, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_MARKET: OrderType.StopLoss,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
};

// export enum OrderStatus {
//   Open = 'open',
//   Closed = 'closed',
//   Canceled = 'canceled',
//
//   // local statuses
//   Creating = 'creating',
//   Updating = 'updating',
//   Canceling = 'canceling',
// }
export const ORDER_STATUS: Record<string, OrderStatus> = {
  INIT: OrderStatus.Open,
  PENDING_CREATE: OrderStatus.Open,
  NEW: OrderStatus.Open,
  PARTIALLY_FILLED: OrderStatus.Open,
  PART_FILLED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELED: OrderStatus.Canceled,
  CANCELLED: OrderStatus.Canceled,
  PENDING_CANCEL: OrderStatus.Canceling,
  REJECTED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Canceled,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export type FETCH_ACCOUNT_RESPONSE = {
  code: string;
  msg: string;
  data: {
    account: {
      marginCoin: string;
      coinPrecious: number;
      exUsdtRate: number;
      exBtcRate: number;
      accountNormal: number;
      accountLock: number;
      partPositionNormal: number;
      totalPositionNormal: number;
      achievedAmount: number;
      unrealizedAmount: number;
      totalMarginRate: number;
      totalEquity: number;
      partEquity: number;
      totalCost: number;
      sumMarginRate: number;
      sumOpenRealizedAmount: number;
      canUseTrialFund: number;
      sumMaintenanceMargin: number;
      futureModel: any;
      positionVos?: {
        contractId: number;
        contractName: string;
        contractSymbol: string;
        adlEnabled: boolean;
        positions: {
          id: number;
          uid: number;
          contractId: number;
          positionType: number;
          side: string;
          volume: number;
          openPrice: number;
          avgPrice: number;
          closePrice: number;
          leverageLevel: number;
          openAmount: number;
          holdAmount: number;
          closeVolume: number;
          pendingCloseVolume: number;
          realizedAmount: number;
          historyRealizedAmount: number;
          forceLiquidationVolume: number;
          forceLiquidationPrice: number;
          tradeFee: number;
          capitalFee: number;
          closeProfit: number;
          shareAmount: number;
          freezeLock: number;
          status: number;
          ctime: string;
          mtime: string;
          brokerId: number;
          usingAccountType: number;
          preShareAmount: number;
          marginRate: number;
          reducePrice: number;
          returnRate: number;
          unRealizedAmount: number;
          openRealizedAmount: number;
          positionBalance: number;
          settleProfit: number;
          keepRate: number;
          maxFeeRate: number;
          version: number;
          indexPrice: number;
        }[];
      }[];
    }[];
  };
};

export type FETCH_CONTRACTS_RESPONSE = {
  // "E-ZRX-USDT"
  symbol: string;
  pricePrecision: number;
  side: number;
  maxMarketVolume: number;
  multiplier: number;
  minOrderVolume: number;
  maxMarketMoney: number;
  type: string;
  maxLimitVolume: number;
  maxValidOrder: number;
  multiplierCoin: string;
  minOrderMoney: number;
  maxLimitMoney: number;
  status: number;
}[];

type BITRUE_LEVERAGE_BRACKET = {
  bracket: number;
  initialLeverage: number | string;
  maxPositionValue: number | string;
  minPositionValue: number | string;
  maintMarginRatio: number | string;
};

type BITRUE_LEVERAGE_BRACKET_DATA_ITEM = {
  contractName?: string;
  brackets?: BITRUE_LEVERAGE_BRACKET[];
};

export type FETCH_LEVERAGE_BRACKET_RESPONSE = {
  code: string | number;
  msg: string;
  data?: BITRUE_LEVERAGE_BRACKET_DATA_ITEM | BITRUE_LEVERAGE_BRACKET_DATA_ITEM[];
};

export type FETCH_KLINE_RESPONSE = {
  high: string;
  vol: string;
  last: string;
  low: string;
  rose: string;
  time: number;
}[];

export type ORDER_BOOK_WS_UPDATE = {
  asks: [number, number][];
  buys: [number, number][];
};

// {
//   "amount": "324107702.758137",
//   "close": "0.016916",
//   "data": [],
//   "high": "0.017614",
//   "low": "0.013704",
//   "open": "0.015467",
//   "rose": "0.0936833258",
//   "vol": "20957595342"
// }

export type TICKER_WS_UPDATE = {
  amount: string;
  close: string;
  data: any[];
  high: string;
  low: string;
  open: string;
  rose: string;
  vol: string;
};

// {
//   "code": "0",
//   "msg": "Success",
//   "data": [{
//   "orderId": 1917641,
//   "clientOrderId": "2488514315",
//   "price": 100,
//   "origQty": 10,
//   "origAmount": 10,
//   "executedQty": 1,
//   "avgPrice": 12451,
//   "status": "INIT",
//   "type": "LIMIT",
//   "side": "BUY",
//   "action": "OPEN",
//   "transactTime": 1686717303975
// }
// ]
// }
export type FETCH_ORDERS_RESPONSE = {
  code: string;
  msg: string;
  data: {
    triggerPrice: number;
    // 1 == Last 2 == Mark
    // currently not provided by rest API...
    triggerPriceType?: 1 | 2;
    // 1 == stop loss, 2 == take profit
    triggerType?: 1 | 2;
    orderId: number;
    clientOrderId: string;
    price: number;
    origQty: number;
    origAmount: number;
    executedQty: number;
    avgPrice: number;
    status: string;
    type?: BITRUE_ORDER_TYPE;
    side: 'BUY' | 'SELL';
    action: string;
    transactTime: number;
  }[];
};

export type FETCH_MY_TRADES_RESPONSE = {
  code: string | number;
  msg: string;
  data: {
    tradeId: number | string;
    price: number | string;
    qty: number | string;
    amount: number | string;
    contractName: string;
    side: 'BUY' | 'SELL' | string;
    fee?: number | string;
    bidId?: number | string;
    askId?: number | string;
    bidUserId?: number | string;
    askUserId?: number | string;
    isBuyer?: boolean;
    isMaker?: boolean;
    ctime?: number | string;
    cTime?: number | string;
    time?: number | string;
  }[];
};

// {
//   "E": 1723870402183,
//   "e": "ORDER_TRADE_UPDATE",
//   "o": {
//   "L": "58777.5",
//     "N": "USDT",
//     "R": false,
//     "S": "BUY",
//     "T": 0,
//     "X": "INIT",
//     "ap": "0",
//     "c": "2334151155877155281",
//     "i": 2334151155877155300,
//     "l": "2.0000000000000000",
//     "m": false,
//     "n": "0",
//     "o": "LIMIT",
//     "p": "58777.5",
//     "q": "0.00020000000000000000000000000000",
//     "s": "E_BTCUSDT",
//     "t": 0,
//     "x": "NEW",
//     "z": "0.0000000000000000"
// }
// }
export type WS_ORDER_TRADE_UPDATE = {
  E: number;
  e: string;
  o: {
    L: string;
    N: string;
    R: boolean;
    S: string;
    T: number;
    X:
      | 'INIT'
      | 'NEW'
      | 'PENDING_CREATE'
      | 'PART_FILLED'
      | 'PARTIALLY_FILLED'
      | 'FILLED'
      | 'PENDING_CANCEL'
      | 'CANCELED'
      | 'CANCELLED'
      | string;
    ap: string;
    c: string;
    i: string | number;
    l: string;
    volume?: number;
    m: boolean;
    n: string;
    o: BITRUE_ORDER_TYPE;
    p: string;
    q: string;
    s: string;
    t: number;
    x: 'NEW' | 'TRADE' | 'CANCEL' | string;
    z: string;
  };
};
