export const BASE_URLS = {
  PRODUCTION_MDG: 'https://market-data.grvt.io',
  PRODUCTION_TDG: 'https://trades.grvt.io',
  PRODUCTION_EDGE: 'https://edge.grvt.io',
  TESTNET_MDG: 'https://market-data.testnet.grvt.io',
  TESTNET_TDG: 'https://trades.testnet.grvt.io',
  TESTNET_EDGE: 'https://edge.testnet.grvt.io',
  STAGING_MDG: 'https://market-data.staging.gravitymarkets.io',
  STAGING_TDG: 'https://trades.staging.gravitymarkets.io',
  STAGING_EDGE: 'https://edge.staging.gravitymarkets.io',
  DEV_MDG: 'https://market-data.dev.gravitymarkets.io',
  DEV_TDG: 'https://trades.dev.gravitymarkets.io',
  DEV_EDGE: 'https://edge.dev.gravitymarkets.io',
};

export const WSS_URLS = {
  PRODUCTION: 'wss://market-data.grvt.io/ws/full',
  TESTNET: 'wss://market-data.testnet.grvt.io/ws/full',
  STAGING: 'wss://market-data.stg.gravitymarkets.io/ws/full',
  DEV: 'wss://market-data.dev.gravitymarkets.io/ws/full',
};

export const ENDPOINTS = {
  // Market Data Gateway (MDG) - Public
  INSTRUMENTS: '/lite/v1/all_instruments',
  INSTRUMENT: '/lite/v1/instrument',
  TICKER: '/lite/v1/ticker',
  MINI_TICKER: '/lite/v1/mini',
  ORDERBOOK: '/lite/v1/book',
  TRADES: '/lite/v1/trade_history',
  KLINES: '/lite/v1/kline',
  FUNDING: '/lite/v1/funding',

  // Edge Gateway - Authentication
  AUTH: '/auth/api_key/login',
  ACCOUNT_SUMMARY: '/lite/v1/account_summary',
  POSITIONS: '/lite/v1/positions',
  OPEN_ORDERS: '/lite/v1/open_orders',
  CREATE_ORDER: '/lite/v1/create_order',
  CANCEL_ORDER: '/lite/v1/cancel_order',
  CANCEL_ALL_ORDERS: '/lite/v1/cancel_all_orders',
  ORDER_HISTORY: '/lite/v1/order_history',
  FILL_HISTORY: '/lite/v1/fill_history',
  GET_LEVERAGE: '/lite/v1/get_all_initial_leverage',
  SET_LEVERAGE: '/lite/v1/set_initial_leverage',

  // Builder Codes
  AUTHORIZE_BUILDER: '/full/v1/authorize_builder',
  GET_AUTHORIZED_BUILDERS: '/full/v1/get_authorized_builders',
  BUILDER_FILL_HISTORY: '/full/v1/builder_fill_history',
};

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.CREATE_ORDER, ENDPOINTS.CANCEL_ORDER, ENDPOINTS.CANCEL_ALL_ORDERS];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.ACCOUNT_SUMMARY, ENDPOINTS.POSITIONS, ENDPOINTS.OPEN_ORDERS];

// GRVT Order Types
export type GrvtOrderSide = 'BUY' | 'SELL';
export type GrvtOrderType = 'LIMIT' | 'MARKET';
export type GrvtTimeInForce = 'GOOD_TILL_TIME' | 'ALL_OR_NONE' | 'IMMEDIATE_OR_CANCEL' | 'FILL_OR_KILL';

// GRVT Candlestick Intervals
export enum GrvtCandlestickInterval {
  CI_1_M = 'CI_1_M',
  CI_3_M = 'CI_3_M',
  CI_5_M = 'CI_5_M',
  CI_15_M = 'CI_15_M',
  CI_30_M = 'CI_30_M',
  CI_1_H = 'CI_1_H',
  CI_2_H = 'CI_2_H',
  CI_4_H = 'CI_4_H',
  CI_6_H = 'CI_6_H',
  CI_8_H = 'CI_8_H',
  CI_12_H = 'CI_12_H',
  CI_1_D = 'CI_1_D',
  CI_3_D = 'CI_3_D',
  CI_5_D = 'CI_5_D',
  CI_1_W = 'CI_1_W',
  CI_2_W = 'CI_2_W',
  CI_3_W = 'CI_3_W',
  CI_4_W = 'CI_4_W',
}
export type GrvtOrderStatus = 'PENDING' | 'OPEN' | 'FILLED' | 'REJECTED' | 'CANCELLED';
export type GrvtAssetKind = 'PERPETUAL' | 'FUTURE' | 'CALL' | 'PUT';

// GRVT API Response Types (Full Format)
export interface GrvtInstrument {
  instrument_id: string;
  instrument_hash: string;
  base: string;
  quote: string;
  kind: GrvtAssetKind;
  venue: string;
  settlement_period: string;
  tick_size: string;
  min_size: string;
  max_size: string;
  step_size: string;
  mark_price: string;
  index_price: string;
  last_trade_price: string;
  last_trade_time: string;
  is_active: boolean;
}

// Lite API format (shortened field names)
export interface GrvtInstrumentLite {
  i: string; // instrument_id
  ih: string; // instrument_hash
  b: string; // base
  q: string; // quote
  k: GrvtAssetKind; // kind
  v?: string[]; // venues
  sp1?: string; // settlement_period
  bd?: number; // base_decimals
  qd?: number; // quote_decimals
  ts: string; // tick_size
  ms: string; // min_size (or could be max, need to verify)
  ss?: string; // step_size
  mp?: string; // mark_price
  ip?: string; // index_price
  ltp?: string; // last_trade_price
  ltt?: string; // last_trade_time
  ia?: boolean; // is_active
  ct?: string; // created_time
  fi?: number; // funding_interval
  af?: string; // admin_fee
  af1?: string; // admin_fee_1
}

export interface GrvtTicker {
  instrument_id: string;
  last_price: string;
  bid_price: string;
  ask_price: string;
  volume_24h: string;
  price_change_24h: string;
  price_change_pct_24h: string;
  high_24h: string;
  low_24h: string;
  funding_rate?: string;
  next_funding_time?: string;
  mark_price: string;
  index_price: string;
  open_interest?: string;
  timestamp: string;
}

export interface GrvtOrderBookLevel {
  p: string; // price
  s: string; // size
  n: string; // number of orders
}

export interface GrvtOrderBook {
  instrument_id: string;
  bids: GrvtOrderBookLevel[];
  asks: GrvtOrderBookLevel[];
  timestamp: string;
}

export interface GrvtTrade {
  trade_id: string;
  instrument_id: string;
  price: string;
  size: string;
  side: GrvtOrderSide;
  timestamp: string;
}

export interface GrvtKline {
  timestamp: string;
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
}

export interface GrvtPosition {
  sub_account_id: string;
  instrument_id: string;
  size: string;
  notional: string;
  entry_price: string;
  mark_price: string;
  unrealized_pnl: string;
  realized_pnl: string;
  total_pnl: string;
  roi: string;
  leverage: string;
  liquidation_price: string;
  margin: string;
  timestamp: string;
}

export interface GrvtBalance {
  currency: string;
  total: string;
  available: string;
  reserved: string;
}

export interface GrvtAccountSummary {
  sub_account_id: string;
  balances: GrvtBalance[];
  total_collateral: string;
  total_notional: string;
  total_upnl: string;
  account_leverage: string;
  margin_usage: string;
  is_liquidatable: boolean;
  timestamp: string;
}

export interface GrvtOrder {
  order_id: string;
  client_order_id?: string;
  sub_account_id: string;
  instrument_id: string;
  side: GrvtOrderSide;
  type: GrvtOrderType;
  time_in_force: GrvtTimeInForce;
  price: string;
  size: string;
  filled_size: string;
  remaining_size: string;
  average_fill_price: string;
  status: GrvtOrderStatus;
  post_only: boolean;
  reduce_only: boolean;
  created_at: string;
  updated_at: string;
  expires_at?: string;
}

export interface GrvtFill {
  fill_id: string;
  trade_id: string;
  order_id: string;
  sub_account_id: string;
  instrument_id: string;
  side: GrvtOrderSide;
  price: string;
  size: string;
  fee: string;
  fee_currency: string;
  liquidity: 'MAKER' | 'TAKER';
  timestamp: string;
}

// Authentication
export interface GrvtAuthRequest {
  api_key: string;
  signature: string;
  nonce: string;
  expires_at: string;
}

export interface GrvtAuthResponse {
  session_token: string;
  account_id: string;
  sub_account_id: string;
  expires_at: string;
}

// WebSocket Message Types
export interface GrvtWebSocketSubscription {
  jsonrpc: '2.0';
  id: number;
  method: 'subscribe' | 'unsubscribe';
  params: {
    channel: string;
    instrument_id?: string;
    interval?: string;
  };
}

export interface GrvtWebSocketMessage {
  jsonrpc: '2.0';
  method?: string;
  params?: any;
  result?: any;
  error?: {
    code: number;
    message: string;
    data?: any;
  };
}
