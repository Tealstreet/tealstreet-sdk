import { OrderSide, OrderType, OrderTimeInForce } from '../../../types';

export const RECV_WINDOW = 20_000;

export const BASE_URL = {
  livenet: 'https://api.prod.paradex.trade/v1',
  testnet: 'https://api.testnet.paradex.trade/v1',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2',
    testnet: 'wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2',
  },
  private: {
    livenet: 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2',
    testnet: 'wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2',
  },
};

export const ENDPOINTS = {
  // Authentication
  AUTH: '/auth',

  // Account
  ACCOUNT: '/account',
  FILLS: '/fills',
  ONBOARDING: '/onboarding',

  // Markets
  MARKETS: '/markets',
  TICKERS: '/markets/summary', // Requires ?market=ALL query param for all tickers

  // Orders
  ORDERS: '/orders',
  ORDERS_HISTORY: '/orders-history',
  ORDER: '/orders/{id}',
  MODIFY_ORDER: '/orders/{id}',
  CANCEL_ORDER: '/orders/{id}', // DELETE method
  CANCEL_ORDER_BY_CLIENT_ID: '/orders/by_client_id/{client_id}', // DELETE method
  CANCEL_ORDERS_BATCH: '/orders/batch', // DELETE method
  CANCEL_ALL_ORDERS: '/orders', // DELETE method with optional ?market= param
  PLACE_ORDER: '/orders',
  PLACE_ORDERS_BATCH: '/orders/batch',

  // Positions
  POSITIONS: '/positions',
  POSITION: '/positions/{market}',

  // Trading
  ORDERBOOK: '/orderbook/{market}',
  OHLCV: '/markets/klines', // Public endpoint, no auth required (limited to 1,3,5,15,30,60 minute intervals)
  TRADINGVIEW_HISTORY: '/tradingview/history', // TradingView-specific endpoint (supports 1D, 4H, etc.)

  // Margin/Leverage settings
  MARGIN: '/account/margin/{market}',
  ALL_MARGINS: '/account/margin', // Get all margin configs
  SET_MARGIN: '/account/margin/{market}',

  // Max slippage settings
  MAX_SLIPPAGE: '/account/profile/market_max_slippage/{market}',
};

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_LIMIT: OrderType.StopLoss,
  STOP_MARKET: OrderType.StopLoss,
  STOP_LOSS_LIMIT: OrderType.StopLoss,
  STOP_LOSS_MARKET: OrderType.StopLoss,
  TAKE_PROFIT_LIMIT: OrderType.TakeProfit,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
};

export const isParadexTriggerOrderType = (type: string): boolean =>
  [
    'STOP_LIMIT',
    'STOP_MARKET',
    'STOP_LOSS_LIMIT',
    'STOP_LOSS_MARKET',
    'TAKE_PROFIT_LIMIT',
    'TAKE_PROFIT_MARKET',
  ].includes(type);

export const isParadexMarketOrderType = (type: string): boolean =>
  ['MARKET', 'STOP_MARKET', 'STOP_LOSS_MARKET', 'TAKE_PROFIT_MARKET'].includes(type);

export const paradexTimeInForce = (row: { instruction?: string; time_in_force?: string; post_only?: boolean }): OrderTimeInForce | undefined => {
  if (row.post_only || row.instruction === 'POST_ONLY') return OrderTimeInForce.PostOnly;
  const instruction = row.instruction || row.time_in_force;
  if (instruction === 'GTC' || instruction === 'GTT') return OrderTimeInForce.GoodTillCancel;
  if (instruction === 'IOC') return OrderTimeInForce.ImmediateOrCancel;
  if (instruction === 'FOK') return OrderTimeInForce.FillOrKill;
  return undefined;
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

// Response types
export type ACCOUNT_RESPONSE = {
  account: string;
  account_number: string;
  address: string;
  public_key: string;
  initial_margin_requirement: string;
  maintenance_margin_requirement: string;
  account_value: string; // Total account value (equity = collateral + upnl)
  total_collateral: string; // Total deposited collateral
  free_collateral: string; // Available collateral for new positions
  margin_cushion: string;
  settlement_asset: string;
  updated_at: number;
  status: string;
  seq_no: number;
};

export type BALANCE_RESPONSE = {
  asset: string;
  free: string;
  locked: string;
  wallet_balance: string;
  unrealized_pnl: string;
  margin_balance: string;
};

export type MARKET_RESPONSE = {
  market?: string; // Some API versions use 'market'
  symbol?: string; // Some API versions use 'symbol'
  status?: 'ACTIVE' | 'INACTIVE';
  base_currency: string;
  quote_currency: string;
  settlement_currency?: string;

  // Order size parameters (different field names in different API versions)
  step_size?: string;
  order_size_increment?: string;

  // Price tick parameters
  tick_size?: string;
  price_tick_size?: string;

  // Pricing
  index_price?: string;
  oracle_price?: string;

  // Order limits
  min_order_size?: string;
  min_notional?: string;
  max_order_size?: string;
  max_position_size?: string;
  position_limit?: string;

  // Margin parameters
  initial_margin_fraction?: string;
  maintenance_margin_fraction?: string;
  baseline_position_size?: string;
  incremental_position_size?: string;
  incremental_initial_margin_fraction?: string;
  delta1_cross_margin_params?: {
    imf_base?: string; // Initial Margin Fraction Base (e.g., "0.02" = 2% = 50x max leverage)
    imf_factor?: string;
    imf_shift?: string;
    mmf_factor?: string; // Maintenance Margin Factor
  };

  // Fees
  maker_fee?: string;
  taker_fee?: string;

  // Options-specific fields
  asset_kind?: string;
  market_kind?: string;
  option_type?: string;
  strike_price?: string;
  expiry_at?: number;
  open_at?: number;

  // Other trading parameters
  max_open_orders?: number;
  price_bands_width?: string;
  max_slippage?: string;
};

export type ORDER_RESPONSE = {
  id: string;
  client_id?: string;
  client_order_id?: string;
  market: string;
  side: 'BUY' | 'SELL';
  size: string;
  remaining_size: string;
  price: string;
  average_fill_price: string;
  status: 'NEW' | 'OPEN' | 'PENDING' | 'FILLED' | 'CANCELED' | 'UNTRIGGERED' | 'REJECTED';
  type:
    | 'LIMIT'
    | 'MARKET'
    | 'STOP_LIMIT'
    | 'STOP_MARKET'
    | 'STOP_LOSS_LIMIT'
    | 'STOP_LOSS_MARKET'
    | 'TAKE_PROFIT_LIMIT'
    | 'TAKE_PROFIT_MARKET';
  instruction?: 'GTC' | 'POST_ONLY' | 'IOC' | 'RPI';
  flags?: string[];
  stp?: 'EXPIRE_MAKER' | 'EXPIRE_TAKER' | 'EXPIRE_BOTH';
  on_behalf_of_account?: string;
  time_in_force?: 'GTT' | 'FOK' | 'IOC';
  created_at: string;
  updated_at: string;
  expires_at: string;
  trigger_price?: string;
  post_only?: boolean;
  reduce_only?: boolean;
};

export type BATCH_ORDER_ERROR = {
  error?: string;
  message?: string;
};

export type BATCH_ORDER_RESPONSE = {
  errors?: BATCH_ORDER_ERROR[];
  orders?: ORDER_RESPONSE[];
};

export type BATCH_CANCEL_ORDER_RESULT = {
  account?: string;
  client_id?: string;
  id?: string;
  market?: string;
  status?: 'QUEUED_FOR_CANCELLATION' | 'ALREADY_CLOSED' | 'NOT_FOUND';
};

export type BATCH_CANCEL_ORDER_RESPONSE = {
  results?: BATCH_CANCEL_ORDER_RESULT[];
};

export type POSITION_RESPONSE = {
  id: string;
  account: string;
  market: string;
  status: 'OPEN' | 'CLOSED' | 'LIQUIDATED';
  side: 'LONG' | 'SHORT';
  size: string;
  average_entry_price: string;
  average_entry_price_usd: string;
  average_exit_price: string;
  unrealized_pnl: string;
  unrealized_funding_pnl: string;
  cost: string;
  cost_usd: string;
  cached_funding_index: string;
  last_updated_at: number;
  created_at: number;
  last_fill_id: string;
  seq_no: number;
  liquidation_price: string;
  leverage: string;
  realized_positional_pnl: string;
  realized_positional_funding_pnl: string;
};

export type TICKER_RESPONSE = {
  symbol: string;
  mark_price: string;
  last_traded_price: string;
  bid: string;
  ask: string;
  volume_24h: string;
  open_interest: string;
  funding_rate: string;
  price_change_rate_24h: string;
  underlying_price?: string;
  delta?: string;
  mark_iv?: string;
  greeks?: {
    delta: string;
    gamma: string;
    vega: string;
    rho?: string;
    vanna?: string;
    volga?: string;
  };
};

export type ORDERBOOK_RESPONSE = {
  bids: Array<[string, string]>; // [price, size]
  asks: Array<[string, string]>; // [price, size]
  timestamp: number;
};

export type TRADE_RESPONSE = {
  id: string;
  price: string;
  size: string;
  side: 'BUY' | 'SELL';
  created_at: string;
};

export type MARGIN_CONFIG_ENTRY = {
  market: string;
  leverage: number;
  margin_type: 'CROSS' | 'ISOLATED';
  isolated_margin_leverage: number;
};

export type MARGIN_RESPONSE = {
  account: string;
  margin_methodology: string;
  configs: MARGIN_CONFIG_ENTRY[];
};

export type WS_MESSAGE_TYPE =
  | 'subscribe'
  | 'unsubscribe'
  | 'ping'
  | 'pong'
  | 'error'
  | 'trades'
  | 'orderbook'
  | 'ticker'
  | 'orders'
  | 'positions'
  | 'balances';
