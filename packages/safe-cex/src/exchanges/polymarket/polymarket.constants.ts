import { OrderSide, OrderType } from '../../types';

/**
 * Polymarket Configuration
 *
 * Polymarket operates on Polygon mainnet (Chain ID 137).
 * It uses EIP-712 signatures for order authentication.
 */
export const POLYMARKET_CONFIG = {
  /** CLOB REST API base URL */
  REST_BASE: 'https://clob.polymarket.com',

  /** CLOB WebSocket for order book and trading data */
  WS_CLOB: 'wss://ws-subscriptions-clob.polymarket.com',

  /** Live data WebSocket for broader market data */
  WS_LIVE: 'wss://ws-live-data.polymarket.com',

  /** Gamma API for market metadata and indexing */
  GAMMA_API: 'https://gamma-api.polymarket.com',

  /** Polygon Chain ID */
  CHAIN_ID: 137,

  /** Contract addresses */
  CONTRACTS: {
    /** CTF Exchange contract */
    EXCHANGE: '0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E',
    /** USDC on Polygon */
    USDC: '0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174',
    /** Conditional Tokens Framework */
    CTF: '0x4D97DCd97eC945f40cF65F87097ACe5EA0476045',
  },

  /** Fee configuration */
  FEES: {
    /** Default fee rate in basis points */
    DEFAULT_FEE_BPS: '0',
    /** Taker fee in basis points */
    TAKER_FEE_BPS: 0,
    /** Maker fee in basis points (usually rebate) */
    MAKER_FEE_BPS: 0,
  },
} as const;

/**
 * REST API Endpoints
 */
export const ENDPOINTS = {
  // Market data (L1 - no auth required)
  MARKETS: '/markets',
  SIMPLIFIED_MARKETS: '/simplified-markets',
  ORDER_BOOK: '/book',
  PRICE: '/price',
  PRICES: '/prices',
  MIDPOINT: '/midpoint',
  MIDPOINTS: '/midpoints',
  SPREAD: '/spread',
  SPREADS: '/spreads',
  LAST_TRADE_PRICE: '/last-trade-price',
  PRICE_HISTORY: '/prices-history',

  // Orders (L2 - auth required)
  ORDER: '/order',
  ORDERS: '/orders',
  CANCEL: '/cancel',
  CANCEL_ORDERS: '/cancel-orders',
  CANCEL_ALL: '/cancel-all',
  CANCEL_MARKET_ORDERS: '/cancel-market-orders',

  // User data (L2 - auth required)
  OPEN_ORDERS: '/orders',
  TRADES: '/trades',
  POSITIONS: '/positions', // Note: This returns from Gamma API

  // Auth
  DERIVE_API_KEY: '/auth/derive-api-key',
  DELETE_API_KEY: '/auth/api-key',
  API_KEYS: '/auth/api-keys',
  NONCE: '/auth/nonce',

  // Account
  BALANCE_ALLOWANCE: '/balance-allowance',
} as const;

/**
 * Order type mappings
 *
 * Polymarket order types:
 * - FOK: Fill-Or-Kill (market order, must fill entirely or cancel)
 * - FAK: Fill-And-Kill (partial fill allowed, unfilled portion cancelled)
 * - GTC: Good-Till-Cancelled (limit order)
 * - GTD: Good-Till-Date (limit order with expiration)
 */
export const ORDER_TYPE_MAP: Record<string, OrderType> = {
  GTC: OrderType.Limit,
  GTD: OrderType.Limit,
  FOK: OrderType.Market,
  FAK: OrderType.Market,
};

export const ORDER_TYPE_REVERSE: Record<OrderType, string> = {
  [OrderType.Limit]: 'GTC',
  [OrderType.Market]: 'FOK',
  [OrderType.Conditional]: 'GTC', // Not supported, fallback
  [OrderType.StopLoss]: 'GTC', // Not supported, fallback
  [OrderType.TakeProfit]: 'GTC', // Not supported, fallback
  [OrderType.TrailingStopLoss]: 'GTC', // Not supported, fallback
};

/**
 * Order side mappings
 */
export const ORDER_SIDE_MAP: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const ORDER_SIDE_REVERSE: Record<OrderSide, string> = {
  [OrderSide.Buy]: 'BUY',
  [OrderSide.Sell]: 'SELL',
};

/**
 * Order status mappings
 */
export const ORDER_STATUS_MAP: Record<string, string> = {
  live: 'open',
  matched: 'closed',
  cancelled: 'canceled',
  delayed: 'open',
};

/**
 * Signature types for Polymarket orders
 *
 * 0: EOA - Standard Ethereum wallet (MetaMask, hardware wallets)
 * 1: POLY_PROXY - Magic.link or email-based wallets
 * 2: POLY_GNOSIS_SAFE - Gnosis Safe multisig
 */
export enum SignatureType {
  EOA = 0,
  POLY_PROXY = 1,
  POLY_GNOSIS_SAFE = 2,
}

/**
 * EIP-712 domain for Polymarket CLOB orders
 */
export const EIP712_DOMAIN = {
  name: 'Polymarket CTF Exchange',
  version: '1',
  chainId: POLYMARKET_CONFIG.CHAIN_ID,
  verifyingContract: POLYMARKET_CONFIG.CONTRACTS.EXCHANGE,
} as const;

/**
 * EIP-712 type definitions for orders
 * Note: Not using `as const` to allow ethers.js TypedDataField[] compatibility
 */
export const EIP712_ORDER_TYPES: Record<string, Array<{ name: string; type: string }>> = {
  Order: [
    { name: 'salt', type: 'uint256' },
    { name: 'maker', type: 'address' },
    { name: 'signer', type: 'address' },
    { name: 'taker', type: 'address' },
    { name: 'tokenId', type: 'uint256' },
    { name: 'makerAmount', type: 'uint256' },
    { name: 'takerAmount', type: 'uint256' },
    { name: 'expiration', type: 'uint256' },
    { name: 'nonce', type: 'uint256' },
    { name: 'feeRateBps', type: 'uint256' },
    { name: 'side', type: 'uint8' },
    { name: 'signatureType', type: 'uint8' },
  ],
};

/**
 * WebSocket message types
 */
export const WS_MESSAGE_TYPES = {
  // Subscription types
  USER: 'USER',
  MARKET: 'MARKET',

  // Operations
  SUBSCRIBE: 'subscribe',
  UNSUBSCRIBE: 'unsubscribe',

  // Events
  PRICE_CHANGE: 'price_change',
  ORDER_BOOK_UPDATE: 'order_book_update',
  TRADE: 'trade',
  ORDER_UPDATE: 'order_update',
  POSITION_UPDATE: 'position_update',
} as const;

/**
 * WebSocket reconnection configuration
 */
export const WS_CONFIG = {
  /** Ping interval in milliseconds */
  PING_INTERVAL: 30000,

  /** Pong timeout in milliseconds */
  PONG_TIMEOUT: 10000,

  /** Reconnect delay in milliseconds */
  RECONNECT_DELAY: 1000,

  /** Maximum reconnect attempts */
  MAX_RECONNECT_ATTEMPTS: 10,
} as const;

/**
 * Rate limiting configuration
 */
export const RATE_LIMITS = {
  /** Requests per second for public endpoints */
  PUBLIC_RPS: 10,

  /** Requests per second for private endpoints */
  PRIVATE_RPS: 5,

  /** Orders per second */
  ORDERS_PER_SECOND: 5,
} as const;

/**
 * Timeframe mappings for OHLCV data
 *
 * Note: Polymarket has limited historical data support
 */
export const TIMEFRAME_MAP: Record<string, string> = {
  '1m': '1m',
  '5m': '5m',
  '15m': '15m',
  '1h': '1h',
  '4h': '4h',
  '1d': '1d',
};
