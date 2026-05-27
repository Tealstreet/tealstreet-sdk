import { OrderSide, OrderStatus, OrderTimeInForce, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://api.direct.bitmex.com/api/v1',
  testnet: 'https://testnet.bitmex.com/api/v1',
};
export const UDF_BASE_URL = {
  livenet: 'https://www.bitmex.com/api',
  testnet: 'https://testnet.bitmex.com/api',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://ws.bitmex.com/realtime',
    testnet: 'wss://ws.testnet.bitmex.com/realtime',
  },
  private: {
    livenet: 'wss://ws.bitmex.com/realtime',
    testnet: 'wss://ws.testnet.bitmex.com/realtime',
  },
};

export const ENDPOINTS = {
  BALANCE: '/user/margin',
  MARKETS: '/instrument/activeAndIndices',
  ACCOUNT: '/user/margin',
  POSITIONS: '/position',
  EXECUTIONS: '/execution/tradeHistory',
  USER_EXECUTION_HISTORY: '/user/executionHistory',
  TICKERS: '/instrument/activeAndIndices',
  SET_LEVERAGE: '/position/leverage', // POST
  SET_ISOLATE: '/position/isolate', // POST
  ORDER: '/order',
  ORDERBOOK: '/orderBook/L2',
  KLINE: '/udf/history',
  WALLET_ASSETS: '/wallet/assets',
};

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.ORDER, ENDPOINTS.SET_LEVERAGE];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.BALANCE,
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.WALLET_ASSETS,
];

export const ORDER_STATUS: Record<string, OrderStatus> = {
  New: OrderStatus.Open,
  PartiallyFilled: OrderStatus.Open,
  Filled: OrderStatus.Closed,
  DoneForDay: OrderStatus.Open,
  Canceled: OrderStatus.Canceled,
  PendingCancel: OrderStatus.Open,
  PendingNew: OrderStatus.Open,
  Rejected: OrderStatus.Canceled,
  Expired: OrderStatus.Canceled,
  Stopped: OrderStatus.Open,
  Untriggered: OrderStatus.Open,
  Triggered: OrderStatus.Open,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  Buy: OrderSide.Buy,
  Sell: OrderSide.Sell,
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  Long: PositionSide.Long,
  Short: PositionSide.Short,
};

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GoodTillCancel: OrderTimeInForce.GoodTillCancel,
  ImmediateOrCancel: OrderTimeInForce.ImmediateOrCancel,
  FillOrKill: OrderTimeInForce.FillOrKill,
  ParticipateDoNotInitiate: OrderTimeInForce.PostOnly,
};

export type Currency = {
  precision: string | undefined;
  majorCurrency: string | undefined;
};
