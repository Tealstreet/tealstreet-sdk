import type { Timeframe } from '../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'e847386590ce4dBC'; // tealstreet

export type OkxRegion = 'okx' | 'okxeu';
export type OkxInstrumentType = 'SWAP' | 'FUTURES';

type OkxRestUrls = { livenet: string; testnet: string };
type OkxWsUrls = {
  public: { livenet: string; testnet: string };
  private: { livenet: string; testnet: string };
  business: { livenet: string; testnet: string };
};

export const BASE_URLS: Record<OkxRegion, OkxRestUrls> = {
  okx: {
    livenet: 'https://www.okx.com',
    // Official OKX Demo REST host. Demo requests still require
    // `x-simulated-trading: 1` and route through the OKX REST proxy path.
    testnet: 'https://openapi.okx.com',
  },
  okxeu: {
    livenet: 'https://eea.okx.com',
    // OKX EU demo keys are entity-scoped and validate on the EEA REST host with
    // `x-simulated-trading: 1`; global `openapi.okx.com` returns 50119 for them.
    testnet: 'https://eea.okx.com',
  },
};

export const BASE_WS_URLS: Record<OkxRegion, OkxWsUrls> = {
  okx: {
    public: {
      livenet: 'wss://ws.okx.com:8443/ws/v5/public',
      testnet: 'wss://wspap.okx.com:8443/ws/v5/public',
    },
    private: {
      livenet: 'wss://ws.okx.com:8443/ws/v5/private',
      testnet: 'wss://wspap.okx.com:8443/ws/v5/private',
    },
    business: {
      livenet: 'wss://ws.okx.com:8443/ws/v5/business',
      testnet: 'wss://wspap.okx.com:8443/ws/v5/business',
    },
  },
  okxeu: {
    public: {
      livenet: 'wss://wseea.okx.com:8443/ws/v5/public',
      testnet: 'wss://wseeapap.okx.com:8443/ws/v5/public',
    },
    private: {
      livenet: 'wss://wseea.okx.com:8443/ws/v5/private',
      testnet: 'wss://wseeapap.okx.com:8443/ws/v5/private',
    },
    business: {
      livenet: 'wss://wseea.okx.com:8443/ws/v5/business',
      testnet: 'wss://wseeapap.okx.com:8443/ws/v5/business',
    },
  },
};

export const ENDPOINTS = {
  ACCOUNT: '/api/v5/account/config',
  ACCOUNT_INSTRUMENTS: '/api/v5/account/instruments',
  PARTNER: '/api/v5/users/partner/if-rebate',
  MARKETS: '/api/v5/public/instruments',
  TICKERS: '/api/v5/market/tickers',
  BALANCE: '/api/v5/account/account-position-risk',
  POSITIONS: '/api/v5/account/positions',
  KLINE: '/api/v5/market/candles',
  KLINE_HISTORY: '/api/v5/market/history-candles',
  // 7 days. There IS a deeper `/orders-history-archive` covering 3 months, and it is
  // NOT usable for this axis: it returns filled orders only. Verified live on one
  // account — `state=canceled` gave 0 rows against the archive and 58 against this
  // route, and a matched 2-day window gave 100/0 filled/canceled there versus 80/20
  // here. Reaching further back therefore costs exactly the orders that never filled,
  // which is the only thing this axis knows that the fills axis does not.
  ORDERS_HISTORY: '/api/v5/trade/orders-history',
  FILLS: '/api/v5/trade/fills',
  FILLS_HISTORY: '/api/v5/trade/fills-history',
  // Closed positions — a DIFFERENT source from BILLS, which is what makes it usable as a
  // cross-check on the ledger rather than a restatement of it.
  POSITIONS_HISTORY: '/api/v5/account/positions-history',
  // BILLS covers the last 7 days, BILLS_ARCHIVE the last 3 months. Windows older
  // than the endpoint's retention answer code=0 with an empty list, not an error.
  BILLS: '/api/v5/account/bills',
  BILLS_ARCHIVE: '/api/v5/account/bills-archive',
  UNFILLED_ORDERS: '/api/v5/trade/orders-pending',
  UNFILLED_ALGO_ORDERS: '/api/v5/trade/orders-algo-pending',
  CANCEL_ORDERS: '/api/v5/trade/cancel-batch-orders',
  CANCEL_ALGO_ORDERS: '/api/v5/trade/cancel-algos',
  UPDATE_ALGO_ORDER: '/api/v5/trade/amend-algos',
  PLACE_ORDERS: '/api/v5/trade/batch-orders',
  PLACE_ALGO_ORDER: '/api/v5/trade/order-algo',
  SET_LEVERAGE: '/api/v5/account/set-leverage',
  LEVERAGE: '/api/v5/account/leverage-info',
  SET_POSITION_MODE: '/api/v5/account/set-position-mode',
  ACCOUNT_CONFIG: '/api/v5/account/config',
  UPDATE_ORDER: '/api/v5/trade/amend-order',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_ALGO_ORDERS,
  ENDPOINTS.UPDATE_ALGO_ORDER,
  ENDPOINTS.PLACE_ORDERS,
  ENDPOINTS.PLACE_ALGO_ORDER,
  ENDPOINTS.UPDATE_ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.LEVERAGE,
  // ENDPOINTS.MARGIN_MODE,
  // ENDPOINTS.POSITION_MODE,
  // ENDPOINTS.SET_MARGIN_MODE,
  ENDPOINTS.SET_POSITION_MODE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.UNFILLED_ORDERS,
  ENDPOINTS.UNFILLED_ALGO_ORDERS,
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
