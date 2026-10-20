import { OrderSide, OrderType } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'ab82cb09-cfec-4473-80a3-b740779d0644'; // tealstreet
export const TESTNET_BROKER_ID = 'e708a644-9ce5-46c6-b50c-74b15f62d8ca'; // safe-cex

export const BASE_URL = {
  livenet: 'https://api-evm.orderly.org',
  testnet: 'https://testnet-api-evm.orderly.org',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://ws-evm.orderly.org/ws/stream',
    testnet: 'wss://testnet-ws-evm.orderly.org/ws/stream',
  },
  private: {
    livenet: 'wss://ws-private-evm.orderly.org/v2/ws/private/stream',
    testnet: 'wss://testnet-ws-private-evm.orderly.org/v2/ws/private/stream',
  },
};

export const ENDPOINTS = {
  ACCOUNT: '/v1/client/info',
  BALANCE: '/v1/client/holding',
  MARKETS: '/v1/public/info',
  TICKERS: '/v1/public/futures',

  POSITIONS: '/v1/positions',
  POSITION: '/v1/position/{symbol}',

  ALGO_ORDERS: '/v1/algo/orders',
  UPDATE_ORDER: '/v1/order',
  ALGO_ORDER: '/v1/algo/order',
  CANCEL_ORDERS: '/v1/orders',
  CANCEL_ALGO_ORDERS: '/v1/algo/orders',
  CANCEL_SYMBOL_ORDERS: '/v1/orders',
  ORDERS: '/v1/orders',
  KLINE: '/v1/kline',
  CANCEL_ORDER: '/v1/order',
  PLACE_ORDER: '/v1/order',
  LEVERAGE: '/v1/client/leverages',
  MARGIN_MODE: '/v1/client/margin_mode',
  MARGIN_MODES: '/v1/client/margin_modes',
  ORDERBOOK: '/v1/orderbook',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ALGO_ORDER,
  ENDPOINTS.POSITION,
  ENDPOINTS.UPDATE_ORDER,
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_ALGO_ORDERS,
  ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.PLACE_ORDER,
  ENDPOINTS.LEVERAGE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.ALGO_ORDERS,
  ENDPOINTS.ORDERS,
  ENDPOINTS.MARGIN_MODES,
];

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  TAKE_PROFIT: OrderType.TakeProfit,
  STOP_LOSS: OrderType.StopLoss,
  TRAILING_STOP: OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export type ACCOUNT_RESPONSE = {
  account_id: string;
  email: string;
  account_mode: 'FUTURES';
  max_leverage: number;
  taker_fee_rate: number;
  maker_fee_rate: number;
  futures_taker_fee_rate: number;
  futures_maker_fee_rate: number;
  maintenance_cancel_orders: boolean;
  // imr_factor: {
  //   PERP_BTC_USDC: 123;
  //   PERP_ETH_USDC: 123;
  //   PERP_NEAR_USDC: 123;
  // };
  // max_notional: {
  //   PERP_BTC_USDC: 123;
  //   PERP_ETH_USDC: 123;
  //   PERP_NEAR_USDC: 123;
  // };
};

export type BALANCE_RESPONSE = {
  holding: ReadonlyArray<{
    updated_time: number;
    token: string;
    holding: number;
    frozen: number;
    pending_short: number;
    availableBalance: number; // am I sure?
  }>;
};

export type PLACE_ORDER_RESPONSE = {
  success: boolean;
  timestamp: string;
  data: {
    order_id: number;
    order_type: string;
    order_price: number;
    order_quantity: number;
    order_amount: number;
    client_order_id: number;
  };
};

export type EDIT_ORDER_REQUEST = {
  order_id: string;
  symbol: string;
  order_type: 'LIMIT';
  side: 'BUY' | 'SELL';
  order_price?: number;
  order_quantity?: number;
};

export type EDIT_ORDER_RESPONSE = {
  success: boolean;
  timestamp?: number;
  code?: number;
  message?: string;
  data?: {
    status?: string;
  };
};
