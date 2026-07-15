// Backpack Exchange Type Definitions

export const BASE_URLS = {
  PRODUCTION: 'https://api.backpack.exchange',
  TESTNET: 'https://api.backpack.exchange', // Backpack doesn't have a separate testnet
};

export const WSS_URLS = {
  PRODUCTION: 'wss://ws.backpack.exchange',
  TESTNET: 'wss://ws.backpack.exchange',
};

export const ENDPOINTS = {
  // Public endpoints
  TIME: '/api/v1/time',
  STATUS: '/api/v1/status',
  ASSETS: '/api/v1/assets',
  MARKETS: '/api/v1/markets',
  MARKET: '/api/v1/market',
  TICKER: '/api/v1/ticker',
  TICKERS: '/api/v1/tickers',
  DEPTH: '/api/v1/depth',
  KLINES: '/api/v1/klines',
  TRADES: '/api/v1/trades',
  TRADES_HISTORY: '/api/v1/trades/history',
  MARK_PRICES: '/api/v1/markPrices',
  OPEN_INTEREST: '/api/v1/openInterest',
  FUNDING_RATES: '/api/v1/fundingRates',

  // Authenticated endpoints
  CAPITAL: '/api/v1/capital',
  CAPITAL_COLLATERAL: '/api/v1/capital/collateral',
  ORDER: '/api/v1/order',
  ORDERS: '/api/v1/orders',
  POSITIONS: '/api/v1/position',
  ACCOUNT: '/api/v1/account',

  // History endpoints
  FILLS_HISTORY: '/wapi/v1/history/fills',
  ORDERS_HISTORY: '/wapi/v1/history/orders',
  DEPOSITS: '/wapi/v1/capital/deposits',
  WITHDRAWALS: '/wapi/v1/capital/withdrawals',
  DEPOSIT_ADDRESS: '/wapi/v1/capital/deposit/address',
};

// Instructions for authenticated requests
export const INSTRUCTIONS = {
  BALANCE_QUERY: 'balanceQuery',
  COLLATERAL_QUERY: 'collateralQuery',
  ORDER_QUERY: 'orderQuery',
  ORDER_QUERY_ALL: 'orderQueryAll',
  ORDER_EXECUTE: 'orderExecute',
  ORDER_CANCEL: 'orderCancel',
  ORDER_CANCEL_ALL: 'orderCancelAll',
  FILL_HISTORY_QUERY_ALL: 'fillHistoryQueryAll',
  ORDER_HISTORY_QUERY_ALL: 'orderHistoryQueryAll',
  POSITION_QUERY: 'positionQuery',
  ACCOUNT_QUERY: 'accountQuery',
  ACCOUNT_UPDATE: 'accountUpdate',
  DEPOSIT_QUERY_ALL: 'depositQueryAll',
  WITHDRAWAL_QUERY_ALL: 'withdrawalQueryAll',
  DEPOSIT_ADDRESS_QUERY: 'depositAddressQuery',
  SUBSCRIBE: 'subscribe',
};

// Grafana endpoints for monitoring
export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.ORDER, ENDPOINTS.ORDERS];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.CAPITAL, ENDPOINTS.POSITIONS];

// Market data types
export interface BackpackAsset {
  blockchain: string;
  depositEnabled: boolean;
  minimumDeposit: string;
  withdrawEnabled: boolean;
  minimumWithdrawal: string;
  maximumWithdrawal: string;
  withdrawalFee: string;
}

export interface BackpackMarket {
  symbol: string;
  baseSymbol: string;
  quoteSymbol: string;
  marketType: 'SPOT' | 'PERP';
  visible: boolean;
  orderBookState: string;
  priceIncrement: string;
  quantityIncrement: string;
  quoteIncrement: string;
  minNotional: string;
  maxNotional: string;
  minQuantity: string;
  maxQuantity: string;
  takerFeeRate?: string;
  makerFeeRate?: string;
  imfFunction?: {
    base: string;
    factor: string;
    type: string;
  };
  mmfFunction?: {
    base: string;
    factor: string;
    type: string;
  };
  // Legacy filters format (for backwards compatibility)
  filters?: {
    price?: {
      minPrice?: string;
      maxPrice?: string;
      tickSize?: string;
    };
    quantity?: {
      minQuantity?: string;
      maxQuantity?: string;
      stepSize?: string;
    };
    leverage?: {
      minLeverage?: number;
      maxLeverage?: number;
    };
  };
}

export interface BackpackTicker {
  symbol: string;
  firstPrice: string;
  lastPrice: string;
  priceChange: string;
  priceChangePercent: string;
  high: string;
  low: string;
  volume: string;
  quoteVolume: string;
  trades: number;
}

export interface BackpackDepth {
  lastUpdateId: number;
  bids: [string, string][]; // [price, quantity]
  asks: [string, string][];
}

export interface BackpackTrade {
  id: number;
  price: string;
  quantity: string;
  quoteQuantity: string;
  timestamp: number;
  isBuyerMaker: boolean;
}

export interface BackpackKline {
  start: string; // ISO date string, e.g., "2025-12-15 13:28:00"
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
  quoteVolume: string;
  end: string; // ISO date string
  trades: string;
}

export interface BackpackMarkPrice {
  symbol: string;
  markPrice: string;
  indexPrice: string;
  fundingRate: string;
  nextFundingTimestamp: number;
}

// Account and trading types
export interface BackpackBalance {
  asset?: string;
  available: string;
  locked: string;
  staked?: string;
}

export interface BackpackOrder {
  id: string;
  clientId?: number;
  symbol: string;
  side: 'Bid' | 'Ask';
  orderType: 'Limit' | 'Market';
  price: string;
  triggerPrice?: string;
  triggerBy?: 'MarkPrice' | 'IndexPrice' | 'LastPrice' | null; // Which price triggers the order
  quantity: string;
  triggerQuantity?: string; // Used for trigger orders (may be returned instead of quantity)
  quoteQuantity?: string;
  timeInForce: 'GTC' | 'IOC' | 'FOK' | 'PO';
  selfTradePrevention: 'RejectTaker' | 'RejectMaker' | 'RejectBoth' | 'Allow';
  status: 'New' | 'PartiallyFilled' | 'Filled' | 'Cancelled' | 'Expired' | 'TriggerPending';
  createdAt: number;
  executedQuantity?: string;
  executedQuoteQuantity?: string;
  reduceOnly?: boolean;
}

export interface BackpackOrderRequest {
  symbol: string;
  side: 'Bid' | 'Ask';
  orderType: 'Limit' | 'Market';
  price?: string;
  triggerPrice?: string;
  quantity?: string;
  triggerQuantity?: string; // Used instead of quantity for trigger orders
  quoteQuantity?: string;
  timeInForce?: 'GTC' | 'IOC' | 'FOK' | 'PO';
  selfTradePrevention?: 'RejectTaker' | 'RejectMaker' | 'RejectBoth' | 'Allow';
  clientId?: number;
  postOnly?: boolean;
  reduceOnly?: boolean;
  // Trigger order settings
  triggerBy?: 'MarkPrice' | 'IndexPrice' | 'LastPrice'; // Which price to monitor for trigger
  // Take Profit attached to order
  takeProfitTriggerPrice?: string;
  takeProfitLimitPrice?: string;
  takeProfitTriggerBy?: 'MarkPrice' | 'IndexPrice' | 'LastPrice';
  // Stop Loss attached to order
  stopLossTriggerPrice?: string;
  stopLossLimitPrice?: string;
  stopLossTriggerBy?: 'MarkPrice' | 'IndexPrice' | 'LastPrice';
}

export interface BackpackFill {
  clientId?: number;
  orderId: string;
  symbol: string;
  side: 'Bid' | 'Ask';
  price: string;
  quantity: string;
  fee: string;
  feeSymbol: string;
  tradeId: number;
  timestamp: string;
  isMaker: boolean;
}

export interface BackpackCollateral {
  assetsValue: string;
  borrowLiability: string;
  collateral: Array<{
    symbol: string;
    assetMarkPrice: string;
    availableQuantity: string;
    balanceNotional: string;
    collateralValue: string;
    collateralWeight: string;
    lendQuantity: string;
    openOrderQuantity: string;
    totalQuantity: string;
  }>;
  imf: string;
  liabilitiesValue: string;
  marginFraction: string;
  mmf: string;
  netEquity: string;
  netEquityAvailable: string;
  netEquityLocked: string;
  netExposureFutures: string;
  pnlUnrealized: string;
  unsettledEquity: string;
}

export interface BackpackPosition {
  symbol: string;
  positionId: string;
  netQuantity: string;
  netExposureQuantity: string;
  netExposureNotional: string;
  netCost: string;
  entryPrice: string;
  markPrice: string;
  breakEvenPrice: string;
  estLiquidationPrice: string;
  pnlUnrealized: string;
  pnlRealized: string;
  cumulativeFundingPayment: string;
  cumulativeInterest: string;
  imf: string;
  mmf: string;
  userId: number;
  subaccountId: string | null;
}

// WebSocket types
export interface BackpackWsSubscribeMessage {
  method: 'SUBSCRIBE';
  params: string[];
  signature?: BackpackWsSignature;
}

export interface BackpackWsUnsubscribeMessage {
  method: 'UNSUBSCRIBE';
  params: string[];
}

export interface BackpackWsSignature {
  key: string;
  signature: string;
  timestamp: number;
  window: number;
}

export interface BackpackWsTickerUpdate {
  e: 'ticker';
  s: string; // symbol
  p: string; // price change
  P: string; // price change percent
  o: string; // open price
  h: string; // high price
  l: string; // low price
  c: string; // last price
  v: string; // volume
  q: string; // quote volume
  E: number; // event time (microseconds)
}

export interface BackpackWsDepthUpdate {
  e: 'depth';
  s: string; // symbol
  b: [string, string][]; // bids
  a: [string, string][]; // asks
  E: number; // event time (microseconds)
  T?: number; // transaction time (microseconds)
  U?: number; // first update ID in event
  u?: number; // final update ID in event
}

export interface BackpackWsTradeUpdate {
  e: 'trade';
  s: string; // symbol
  i: number; // trade id
  p: string; // price
  q: string; // quantity
  m: boolean; // is buyer maker
  E: number; // event time (microseconds)
}

export interface BackpackWsKlineUpdate {
  e: 'kline';
  s: string; // symbol
  i?: string; // interval (not in actual message, only in stream name)
  t: string; // start time (ISO date string, e.g., "2025-12-15T19:02:00")
  T: string; // close time (ISO date string)
  o: string | null; // open (null on empty candle close)
  h: string | null; // high
  l: string | null; // low
  c: string | null; // close
  v: string | null; // volume
  n: number; // number of trades
  X: boolean; // is closed
  E: number; // event time (microseconds)
}

export interface BackpackWsBookTickerUpdate {
  e: 'bookTicker';
  s: string; // symbol
  b: string; // best bid price
  B: string; // best bid quantity
  a: string; // best ask price
  A: string; // best ask quantity
  E: number; // event time (microseconds)
}

// WebSocket order update - actual API format
// Message wrapper: { "stream": "account.orderUpdate", "data": { ... } }
// Event types: orderAccepted, orderFill, orderCancelled, orderExpired, orderModified
export interface BackpackWsOrderUpdate {
  e: string; // event type (orderAccepted, orderFill, orderCancelled, etc.)
  i: string; // order id
  c?: string; // client order id
  E: number; // event time (microseconds)
  T: number; // transaction time (microseconds)
  X: string; // order status (New, PartiallyFilled, Filled, Cancelled, Expired)
  s: string; // symbol/market
  o: string; // order type (Limit, MARKET) - note: can be uppercase
  S: 'Bid' | 'Ask'; // side
  p?: string; // price (not present on market orders)
  P?: string; // trigger price (for trigger/stop orders)
  q: string; // quantity (amount)
  Q?: string; // trigger quantity (for trigger/stop orders)
  l?: string; // last executed quantity (on fills)
  L?: string; // last executed price (on fills)
  z: string; // cumulative executed quantity
  Z?: string; // cumulative quote quantity (cost)
  N?: string; // fee currency
  n?: string; // commission
  f?: string; // time in force (GTC, IOC, FOK, PO)
  r?: boolean; // reduce only
  t?: number | null; // trade id (on fills)
  m?: boolean; // is maker (on fills)
}

// WebSocket position update - actual API format
// Message wrapper: { "stream": "account.positionUpdate", "data": { ... } }
export interface BackpackWsPositionUpdate {
  i: number; // position id
  s: string; // symbol
  b: string; // entry price (breakeven)
  B?: string; // entry price alt
  M: string; // mark price
  n: string; // notional value
  Q: string; // quantity (absolute)
  q: string; // net quantity (negative = short)
  P: string; // unrealized pnl
  p: string; // realized pnl
  l: string; // liquidation price
  m: string; // maintenance margin %
  f: string; // initial margin % (imf)
  E: number; // event time (microseconds)
  T?: number; // transaction time
}

// Error types
export interface BackpackError {
  code: number;
  msg: string;
}

// API response wrappers
export interface BackpackApiResponse<T> {
  data?: T;
  error?: BackpackError;
}

// Time and status
export interface BackpackTime {
  serverTime: number;
}

export interface BackpackStatus {
  status: 'normal' | 'maintenance';
  msg?: string;
}

// Account info
export interface BackpackAccount {
  autoBorrowSettlements: boolean;
  autoLend: boolean;
  autoRealizePnl: boolean;
  autoRepayBorrows: boolean;
  borrowLimit: string;
  futuresMakerFee: string;
  futuresTakerFee: string;
  leverageLimit: string;
  limitOrders: number;
  liquidating: boolean;
  positionLimit: string;
  spotMakerFee: string;
  spotTakerFee: string;
  triggerOrders: number;
}
