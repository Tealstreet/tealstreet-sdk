// Crypto.com Exchange Type Definitions

import type { Timeframe } from '../../../types';

type CryptocomCandlestickInterval = '1m' | '5m' | '15m' | '30m' | '1h' | '2h' | '4h' | '12h' | '1D' | '7D';

const CANDLESTICK_INTERVALS: Partial<Record<Timeframe, CryptocomCandlestickInterval>> = {
  '1m': '1m',
  '5m': '5m',
  '15m': '15m',
  '30m': '30m',
  '1h': '1h',
  '2h': '2h',
  '4h': '4h',
  '12h': '12h',
  '1d': '1D',
  '1w': '7D',
};

export const mapCandlestickInterval = (interval: Timeframe): CryptocomCandlestickInterval => {
  const mapped = CANDLESTICK_INTERVALS[interval];
  if (!mapped) throw new Error(`Unsupported Crypto.com candlestick interval: ${interval}`);
  return mapped;
};

export const BASE_URLS = {
  PRODUCTION: 'https://api.crypto.com/exchange/v1',
  TESTNET: 'https://uat-api.3ona.co/exchange/v1',
};

export const WSS_URLS = {
  PRODUCTION: {
    USER: 'wss://stream.crypto.com/exchange/v1/user',
    MARKET: 'wss://stream.crypto.com/exchange/v1/market',
  },
  TESTNET: {
    USER: 'wss://uat-stream.3ona.co/exchange/v1/user',
    MARKET: 'wss://uat-stream.3ona.co/exchange/v1/market',
  },
};

export const CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS = 30_000;
export const CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS = 90_000;

export const ENDPOINTS = {
  // Public endpoints
  INSTRUMENTS: '/public/get-instruments',
  BOOK: '/public/get-book',
  TICKERS: '/public/get-tickers',
  TICKER: '/public/get-ticker',
  TRADES: '/public/get-trades',
  CANDLESTICK: '/public/get-candlestick',
  VALUATIONS: '/public/get-valuations',
  EXPIRED_SETTLEMENT_PRICE: '/public/get-expired-settlement-price',
  INSURANCE: '/public/get-insurance',

  // Private endpoints
  ACCOUNTS: '/private/get-accounts',
  CREATE_ORDER: '/private/create-order',
  ADVANCED_CREATE_ORDER: '/private/advanced/create-order',
  ADVANCED_AMEND_ORDER: '/private/advanced/amend-order',
  ADVANCED_CANCEL_ORDER: '/private/advanced/cancel-order',
  ADVANCED_OPEN_ORDERS: '/private/advanced/get-open-orders',
  AMEND_ORDER: '/private/amend-order',
  CANCEL_ORDER: '/private/cancel-order',
  CANCEL_ALL_ORDERS: '/private/cancel-all-orders',
  CLOSE_POSITION: '/private/close-position',
  ORDER_DETAIL: '/private/get-order-detail',
  OPEN_ORDERS: '/private/get-open-orders',
  ORDER_HISTORY: '/private/get-order-history',
  TRADES_HISTORY: '/private/get-trades',
  POSITIONS: '/private/get-positions',
  CHANGE_LEVERAGE: '/private/change-account-leverage',
  BALANCE: '/private/user-balance',
  BALANCE_HISTORY: '/private/user-balance-history',
  POSITION_BALANCE: '/private/get-position-balance',
  CREATE_WITHDRAWAL: '/private/create-withdrawal',
  GET_DEPOSIT_ADDRESS: '/private/get-deposit-address',
  GET_WITHDRAWAL_HISTORY: '/private/get-withdrawal-history',
  GET_DEPOSIT_HISTORY: '/private/get-deposit-history',
  GET_CURRENCY_NETWORKS: '/private/get-currency-networks',
  SUBACCOUNT_BALANCES: '/private/get-subaccount-balances',
  CREATE_SUBACCOUNT_TRANSFER: '/private/create-subaccount-transfer',
  GET_SUBACCOUNT_TRANSFER_HISTORY: '/private/get-subaccount-transfer-history',
};

export const PUBLIC_ENDPOINTS = [
  ENDPOINTS.INSTRUMENTS,
  ENDPOINTS.BOOK,
  ENDPOINTS.TICKERS,
  ENDPOINTS.TICKER,
  ENDPOINTS.TRADES,
  ENDPOINTS.CANDLESTICK,
  ENDPOINTS.VALUATIONS,
  ENDPOINTS.EXPIRED_SETTLEMENT_PRICE,
  ENDPOINTS.INSURANCE,
];

// Grafana endpoints for monitoring
export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.CREATE_ORDER,
  ENDPOINTS.AMEND_ORDER,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.CANCEL_ALL_ORDERS,
  ENDPOINTS.CLOSE_POSITION,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNTS,
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.OPEN_ORDERS,
];

// Market data types — public/get-instruments (v1, GET) response item shape.
export interface CryptocomInstrument {
  symbol: string;
  inst_type: string; // e.g. PERPETUAL_SWAP, FUTURE
  display_name: string;
  base_ccy: string;
  quote_ccy: string;
  quote_decimals: number;
  quantity_decimals: number;
  price_tick_size: string;
  qty_tick_size: string;
  max_leverage: string;
  tradable: boolean;
  expiry_timestamp_ms: number;
  beta_product: boolean;
  underlying_symbol: string;
  contract_size: string;
  margin_buy_enabled: boolean;
  margin_sell_enabled: boolean;
}

export interface CryptocomBook {
  instrument_name: string;
  depth: number;
  // v1 GET wire format: [price, quantity, order_count] tuples as strings.
  data: Array<{
    bids: Array<[string, string, string]>;
    asks: Array<[string, string, string]>;
    t: number; // timestamp
  }>;
}

// All numeric fields arrive as strings from the v1 API — parseFloat at the boundary.
export interface CryptocomTicker {
  i: string; // instrument_name
  b: string; // best bid
  k: string; // best ask
  a: string; // last trade price
  c: string; // 24h percentage change as a decimal fraction (e.g. "-0.0071" = -0.71%)
  h: string; // 24h high
  l: string; // 24h low
  v: string; // 24h volume in base currency
  vv: string; // 24h volume in quote currency
  oi: string; // open interest
  t: number; // timestamp (ms)
}

export interface CryptocomTrade {
  d: number; // trade id
  t: number; // timestamp
  p: number; // price
  q: number; // quantity
  s: 'BUY' | 'SELL'; // side
  i: string; // instrument_name
  dataTime: number; // data timestamp
}

// v1 candlestick fields are strings; parseFloat at the boundary.
export interface CryptocomCandlestick {
  t: number; // bucket start time (ms)
  o: string; // open
  h: string; // high
  l: string; // low
  c: string; // close
  v: string; // volume
}

// Account and trading types

// v1 `position_balances[]` item — one row per collateral asset on the account.
// All numeric fields are strings; parseFloat at the boundary.
export interface CryptocomPositionBalance {
  instrument_name: string; // asset symbol, e.g. "USD"
  quantity: string;
  reserved_qty: string;
  collateral_amount: string;
  collateral_eligible: boolean;
  haircut: string;
  market_value: string;
  max_withdrawal_balance: string;
  hourly_interest_rate: string;
}

// v1 `private/user-balance` returns `{ result: { data: [{ ...summary, position_balances: [...] }] } }`.
// The outer summary carries totals; the inner array carries per-asset rows.
export interface CryptocomBalanceSummary {
  total_available_balance: string;
  total_margin_balance: string;
  total_initial_margin: string;
  total_maintenance_margin: string;
  total_cash_balance: string;
  total_collateral_value: string;
  total_session_unrealized_pnl: string;
  total_session_realized_pnl: string;
  total_position_cost: string;
  instrument_name: string; // account quote (e.g. "USD")
  position_balances: CryptocomPositionBalance[];
}

// v1 REST + WS share most field names; numerics are strings. The v0 `order_status`
// is now `status`, and `time_created` / `time_updated` are `create_time` /
// `update_time` (millisecond integers). WS frames also still send the legacy
// `order_status` field on some channels, so the type accepts either.
export interface CryptocomOrder {
  account_id?: string;
  order_id: string;
  client_oid?: string;
  order_type: 'LIMIT' | 'MARKET' | 'STOP_LOSS' | 'STOP_LIMIT' | 'TAKE_PROFIT' | 'TAKE_PROFIT_LIMIT';
  time_in_force: 'GOOD_TILL_CANCEL' | 'FILL_OR_KILL' | 'IMMEDIATE_OR_CANCEL' | 'POST_ONLY';
  side: 'BUY' | 'SELL';
  exec_inst?: string[];
  quantity: string;
  limit_price?: string;
  trigger_price?: string;
  ref_price?: string;
  ref_price_type?: 'MARK_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE';
  avg_price?: string;
  cumulative_quantity?: string;
  cumulative_value?: string;
  fee_currency?: string;
  fee_instrument_name?: string;
  create_time?: number;
  update_time?: number;
  // Legacy field names still seen on some WS frames; read whichever is present.
  time_created?: number;
  time_updated?: number;
  status: 'ACTIVE' | 'FILLED' | 'CANCELED' | 'EXPIRED' | 'PENDING' | 'REJECTED';
  order_status?: 'ACTIVE' | 'FILLED' | 'CANCELED' | 'EXPIRED' | 'PENDING' | 'REJECTED';
  instrument_name: string;
  post_only?: boolean;
}

export interface CryptocomOrderRequest {
  instrument_name: string;
  side: 'BUY' | 'SELL';
  type: 'LIMIT' | 'MARKET' | 'STOP_LOSS' | 'STOP_LIMIT' | 'TAKE_PROFIT' | 'TAKE_PROFIT_LIMIT';
  quantity?: string;
  notional?: string; // MARKET BUY by cost only
  price?: string;
  ref_price?: string;
  ref_price_type?: 'MARK_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE';
  time_in_force?: 'GOOD_TILL_CANCEL' | 'FILL_OR_KILL' | 'IMMEDIATE_OR_CANCEL' | 'POST_ONLY';
  exec_inst?: string[];
  client_oid?: string;
  post_only?: boolean;
}

// v1 REST positions are returned with all numerics as strings. The legacy
// `type: LONG_POSITION | SHORT_POSITION` field is gone — v1 always reports
// `type: "PERPETUAL_SWAP"` and the side is encoded in the sign of `quantity`
// (negative = short). The WS frames may still carry the legacy LONG/SHORT
// type, so the union accepts either form; the side is derived from quantity.
export interface CryptocomPosition {
  account_id?: string;
  quantity: string;
  cost: string;
  open_position_pnl: string;
  session_pnl?: string;
  instrument_name: string;
  open_pos_cost?: string;
  update_timestamp_ms?: number;
  type: 'PERPETUAL_SWAP' | 'LONG_POSITION' | 'SHORT_POSITION';
  position_id?: string;
  leverage?: string;
  liquidation_price?: string;
  mark_price?: string;
  isolation_id?: string;
  isolation_type?: 'ISOLATED_MARGIN';
}

export interface CryptocomAccountsResponse {
  this_account: {
    uuid: string;
  };
}

export interface CryptocomAccountSummary {
  accounts: Array<{
    balance: number;
    available_balance: number;
    margin_balance: number;
    order_margin: number;
    position_margin: number;
    currency: string;
    is_liquidating: boolean;
    position_limit: number;
    used_position_limit: number;
    position_limit_usd: number;
    margin_score: string;
    has_risk: boolean;
    account_id?: string;
  }>;
}

export interface CryptocomTradeFill {
  account_id: string;
  event_date: number;
  journal_type: 'TRADING';
  traded_quantity: number;
  traded_price: number;
  transaction_qty: number;
  transaction_cost: number;
  order_id: string;
  trade_id: string;
  trade_match_id: string;
  client_oid?: string;
  taker_side: 'BUY' | 'SELL';
  side: 'BUY' | 'SELL';
  instrument_name: string;
  fee: number;
  fee_currency: string;
  create_time: number;
  create_time_ns: string;
  liquidity_indicator: 'MAKER' | 'TAKER';
}

// WebSocket types
export interface CryptocomWsRequest {
  id: number;
  method: string;
  params?: Record<string, unknown>;
  api_key?: string;
  sig?: string;
  nonce?: number;
}

export interface CryptocomWsResponse {
  id?: number;
  method?: string;
  code: number;
  message?: string;
  result?: unknown;
}

export interface CryptocomWsSubscription {
  id: number;
  method: 'subscribe';
  params: {
    channels: string[];
  };
  nonce?: number;
}

export interface CryptocomWsTickerUpdate {
  instrument_name: string;
  subscription: string;
  channel: string;
  // Same shape as the REST CryptocomTicker — all numeric fields are strings.
  data: Array<{
    i: string; // instrument_name
    b: string; // best bid
    k: string; // best ask
    a: string; // last trade price
    c: string; // 24h percentage change as a decimal fraction
    h: string; // 24h high
    l: string; // 24h low
    v: string; // 24h volume in base
    vv: string; // 24h volume in quote
    oi?: string; // open interest (present on perp tickers)
    t: number; // timestamp (ms)
  }>;
}

export type CryptocomWsValuationChannel = 'mark' | 'index' | 'funding' | 'estimatedfunding';

// The four public valuation channels share the same v1 payload. `v` remains
// a string on the wire; handlers must validate it before updating a ticker.
export interface CryptocomWsValuationUpdate {
  instrument_name: string;
  subscription: string;
  channel: CryptocomWsValuationChannel;
  data: Array<{
    v: string;
    t: number;
  }>;
}

export interface CryptocomWsTradeUpdate {
  instrument_name: string;
  subscription: string;
  channel: string;
  data: Array<{
    d: number; // trade id
    t: number; // timestamp
    p: number; // price
    q: number; // quantity
    s: 'BUY' | 'SELL'; // side
    i: string; // instrument_name
    dataTime: number;
  }>;
}

export interface CryptocomWsBookUpdate {
  instrument_name: string;
  subscription: string;
  channel: string;
  // [price, quantity, order_count] tuples as strings, matching the REST shape.
  data: Array<{
    bids: Array<[string, string, string]>;
    asks: Array<[string, string, string]>;
    t: number; // timestamp
    tt: number; // trade timestamp
  }>;
}

export interface CryptocomWsCandlestickUpdate {
  instrument_name: string;
  subscription: string;
  channel: string;
  // Numeric candle fields arrive as strings.
  data: Array<{
    t: number; // bucket start time (ms)
    o: string; // open
    h: string; // high
    l: string; // low
    c: string; // close
    v: string; // volume
  }>;
}

// v1 user.order frame. Numerics are strings; `status` replaces `order_status`
// and `create_time`/`update_time` replace `time_created`/`time_updated`. The
// legacy field names are still emitted on some channels, so we accept either.
export interface CryptocomWsOrderUpdate {
  subscription: string;
  channel: string;
  data: Array<{
    account_id?: string;
    order_id: string;
    client_oid?: string;
    order_type: string;
    time_in_force: string;
    side: 'BUY' | 'SELL';
    exec_inst?: string[];
    quantity: string;
    limit_price?: string;
    trigger_price?: string;
    ref_price?: string;
    ref_price_type?: 'MARK_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE';
    avg_price?: string;
    cumulative_quantity?: string;
    cumulative_value?: string;
    fee_currency?: string;
    fee_instrument_name?: string;
    create_time?: number;
    update_time?: number;
    // Legacy v0 field names — still seen on some channels.
    time_created?: number;
    time_updated?: number;
    status?: string;
    order_status?: string;
    instrument_name: string;
    reason?: number;
    update_user_id?: string;
  }>;
}

// v1 user.positions frame. Numerics are strings. `type` is now constant
// `PERPETUAL_SWAP` — side must be derived from the sign of `quantity`. The
// legacy LONG_POSITION/SHORT_POSITION values are still accepted for
// compatibility but never used for routing.
export interface CryptocomWsPositionUpdate {
  subscription: string;
  channel: string;
  data: Array<{
    account_id?: string;
    quantity: string;
    cost: string;
    open_position_pnl: string;
    session_pnl?: string;
    instrument_name: string;
    open_pos_cost?: string;
    update_timestamp_ms?: number;
    type: 'PERPETUAL_SWAP' | 'LONG_POSITION' | 'SHORT_POSITION';
    mark_price?: string;
    target_leverage?: string;
    leverage?: string;
    liquidation_price?: string;
    isolated_margin_balance?: string;
    isolation_id?: string;
    isolation_type?: 'ISOLATED_MARGIN';
  }>;
}

// v1 user.balance frame mirrors the REST `position_balances[]` row shape
// (instrument_name + string numerics). The wrapper at the result level still
// carries `subscription` / `channel`.
export interface CryptocomWsBalanceUpdate {
  subscription: string;
  channel: string;
  data: CryptocomBalanceSummary[];
}

// Error types
export interface CryptocomError {
  code: number;
  message: string;
}

// API response wrapper
export interface CryptocomApiResponse<T> {
  id?: number;
  method: string;
  code: number;
  result?: T;
  message?: string;
}
