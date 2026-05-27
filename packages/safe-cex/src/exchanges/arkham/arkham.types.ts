import { OrderSide, OrderType } from '../../types';

export const RECV_WINDOW = 20_000;

export const BASE_URL = {
  livenet: 'https://arkm.com/api',
  testnet: 'https://arkm.com/api', // No testnet available yet
};

// Arkham uses a unified WebSocket endpoint for both public and private channels
export const BASE_WS_URL = {
  livenet: 'wss://arkm.com/ws',
  testnet: 'wss://arkm.com/ws', // No testnet available yet
};

export const ENDPOINTS = {
  // Account
  ACCOUNT: '/user', // Corrected from /account/info
  BALANCES: '/account/balances',
  POSITIONS: '/account/positions',
  DEPOSITS: '/account/deposits',
  WITHDRAWALS: '/account/withdrawals',

  // Public
  MARKETS: '/public/pairs', // Corrected from /public/markets
  TICKERS: '/public/tickers',
  TICKER: '/public/ticker',
  ORDERBOOK: '/public/book',
  TRADES: '/public/trades',
  KLINE: '/public/candles',

  // Trading
  PLACE_ORDER: '/orders/new',
  PLACE_TRIGGER_ORDER: '/trigger-orders/new',
  CANCEL_ORDER: '/orders/cancel',
  CANCEL_TRIGGER_ORDER: '/trigger-orders/cancel',
  CANCEL_ALL_ORDERS: '/orders/cancel/all', // Corrected from /orders/cancel-all
  CANCEL_ALL_TRIGGER_ORDERS: '/trigger-orders/cancel/all',
  ORDERS: '/orders', // Corrected from /orders/active
  TRIGGER_ORDERS: '/trigger-orders',
  ORDER_HISTORY: '/orders/history',
  ORDER_STATUS: '/orders/status',

  // Leverage (POST to same endpoint sets, GET retrieves)
  LEVERAGE: '/account/leverage',
  SET_LEVERAGE: '/account/leverage',
};

export const ORDER_TYPE: Record<string, OrderType> = {
  limitGtc: OrderType.Limit,
  limitIoc: OrderType.Limit,
  limitFok: OrderType.Limit,
  market: OrderType.Market,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  buy: OrderSide.Buy,
  sell: OrderSide.Sell,
};

// Response types
export type ACCOUNT_RESPONSE = {
  account_id: string;
  email?: string;
  account_type: string;
  tier: number;
  maker_fee_rate: number;
  taker_fee_rate: number;
  arkm_balance: string;
  volume_30d: string;
};

export type BALANCE_RESPONSE = {
  subaccountId: number;
  symbol: string; // Asset symbol (USDT, USDC, BTC, etc.)
  balance: string; // Total balance
  free: string; // Free balance
  priceUSDT: string;
  balanceUSDT: string;
  freeUSDT: string;
  lastUpdateReason: string;
  lastUpdateTime: number;
  lastUpdateId: number;
  lastUpdateAmount: string;
};

export type MARKET_RESPONSE = {
  symbol: string;
  baseSymbol: string;
  baseImageUrl?: string;
  baseIsStablecoin?: boolean;
  baseName?: string;
  quoteSymbol: string;
  quoteImageUrl?: string;
  quoteIsStablecoin?: boolean;
  quoteName?: string;
  minTickPrice: string;
  minLotSize: string;
  minSize: string;
  maxSize: string;
  minPrice: string;
  maxPrice: string;
  minNotional: string;
  maxPriceScalarUp?: string;
  maxPriceScalarDown?: string;
  pairType: 'spot' | 'perp';
  maxLeverage: string;
  status: 'listed' | 'delisted' | 'suspended';
};

export type ORDER_RESPONSE = {
  orderId: number; // Number, not string
  clientOrderId?: string;
  symbol: string;
  subaccountId: number;
  side: 'buy' | 'sell';
  type: 'limitGtc' | 'limitIoc' | 'limitFok' | 'market';
  status?: 'new' | 'taker' | 'booked' | 'maker' | 'cancelled' | 'closed'; // Not included in place order response
  price: string;
  size: string;
  filledSize?: string;
  remainingSize?: string;
  averagePrice?: string;
  time: number; // Timestamp in microseconds
};

export type TRIGGER_ORDER_RESPONSE = {
  triggerOrderId: number; // Note: different field name from regular orders
  subaccountId: number;
  symbol: string;
  side: 'buy' | 'sell';
  type: 'limitGtc' | 'limitIoc' | 'limitFok' | 'market';
  size: string;
  price: string;
  postOnly: boolean;
  reduceOnly: boolean;
  time: number; // Timestamp in microseconds
  triggerType: 'stopLoss' | 'takeProfit';
  triggerPriceType: 'last' | 'mark' | 'index';
  triggerPrice: string;
  clientOrderId: string;
  status: 'staged' | 'triggered' | 'cancelled' | 'failed';
};

export type POSITION_RESPONSE = {
  subaccountId: number;
  symbol: string;
  base: string; // Position size in base currency (positive for long, negative for short)
  quote: string; // Notional value in quote currency
  openBuySize: string;
  openSellSize: string;
  openBuyNotional: string;
  openSellNotional: string;
  lastUpdateReason: string;
  lastUpdateTime: number; // Microseconds
  lastUpdateId: number;
  lastUpdateBaseDelta: string;
  lastUpdateQuoteDelta: string;
  breakEvenPrice: string;
  markPrice: string;
  value: string; // Current position value
  pnl: string; // Unrealized PnL
  initialMargin: string;
  maintenanceMargin: string;
  averageEntryPrice: string;
};

export type TICKER_RESPONSE = {
  symbol: string;
  baseSymbol: string;
  quoteSymbol: string;
  price: string; // Current price (not last_price)
  price24hAgo: string;
  high24h: string;
  low24h: string;
  volume24h: string;
  quoteVolume24h: string;
  markPrice: string;
  indexPrice: string;
  fundingRate: string;
  nextFundingRate: string;
  nextFundingTime: number;
  productType: string; // "spot" or "perp"
  openInterest: string;
  indexCurrency: string;
  usdVolume24h: string;
  openInterestUSD: string;
};

export type ORDERBOOK_RESPONSE = {
  symbol: string;
  bids: Array<[string, string]>; // [price, size]
  asks: Array<[string, string]>; // [price, size]
  timestamp: number;
  sequence: number;
};

export type TRADE_RESPONSE = {
  id: string;
  symbol: string;
  price: string;
  size: string;
  side: 'buy' | 'sell';
  timestamp: number;
};

export type KLINE_RESPONSE = {
  symbol: string;
  time: number; // Timestamp in microseconds
  duration: number; // Duration in microseconds
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
  quoteVolume: string; // Note: quoteVolume not quote_volume
};

export type LEVERAGE_RESPONSE = {
  symbol: string;
  leverage: number;
};

// WebSocket message types
export type WS_CHANNEL =
  | 'candles'
  | 'ticker'
  | 'l2_updates'
  | 'l1_book'
  | 'trades'
  | 'order_statuses'
  | 'fills'
  | 'positions'
  | 'balances'
  | 'margin'
  | 'trigger_orders'
  | 'lsp_assignments';

export type WS_METHOD = 'subscribe' | 'unsubscribe';

export type WS_SUBSCRIPTION = {
  method: WS_METHOD;
  args: {
    channel: WS_CHANNEL;
    params?: {
      symbol?: string;
      duration?: string; // For candles: "1m", "5m", "15m", "30m", "1h", "6h", "24h"
    };
  };
  confirmationId?: string;
};

export type WS_MESSAGE = {
  channel?: string;
  data?: any;
  error?: {
    code: number;
    id: number;
    name: string;
    message: string;
  };
  confirmationId?: string;
};