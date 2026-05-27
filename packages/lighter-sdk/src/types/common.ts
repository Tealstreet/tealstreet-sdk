/**
 * Common types and interfaces used across the Lighter SDK
 */

import { ResultCode } from './constants';

/**
 * Generic API response wrapper
 * @template T - The type of data returned in the response
 */
export interface ApiResponse<T> {
  /** Result code indicating success or failure */
  code: ResultCode;
  /** Human-readable message */
  message: string;
  /** Response data (present on success) */
  data?: T;
  /** Error details (present on failure) */
  error?: string;
  /** Request timestamp */
  timestamp: number;
}

/**
 * SDK configuration options
 */
export interface LighterConfig {
  /** Base URL for API endpoints */
  baseUrl: string;
  /** WebSocket URL for real-time data */
  wsUrl?: string;
  /** API key for authentication */
  apiKey?: string;
  /** API secret for signing requests */
  apiSecret?: string;
  /** Private key for signing transactions */
  privateKey?: string;
  /** Chain ID for the blockchain */
  chainId?: number;
  /** Request timeout in milliseconds */
  timeout?: number;
  /** Enable debug logging */
  debug?: boolean;
  /** Maximum number of retry attempts */
  maxRetries?: number;
  /** Retry delay in milliseconds */
  retryDelay?: number;
}

/**
 * Account information
 */
export interface Account {
  /** Account address */
  address: string;
  /** Account index */
  accountIndex: number;
  /** Public key */
  publicKey: string;
  /** Nonce for transactions */
  nonce: number;
  /** Account balance information */
  balances: Balance[];
  /** Account creation timestamp */
  createdAt: number;
  /** Last update timestamp */
  updatedAt: number;
}

/**
 * Balance information for a token
 */
export interface Balance {
  /** Token symbol */
  symbol: string;
  /** Token address */
  tokenAddress: string;
  /** Available balance */
  available: string;
  /** Locked balance (in orders) */
  locked: string;
  /** Total balance */
  total: string;
  /** Token decimals */
  decimals: number;
}

/**
 * Market information
 */
export interface Market {
  /** Market ID */
  marketId: string;
  /** Market symbol (e.g., "BTC-USDT") */
  symbol: string;
  /** Base token symbol */
  baseToken: string;
  /** Quote token symbol */
  quoteToken: string;
  /** Base token address */
  baseTokenAddress: string;
  /** Quote token address */
  quoteTokenAddress: string;
  /** Minimum order size */
  minOrderSize: string;
  /** Maximum order size */
  maxOrderSize: string;
  /** Price tick size */
  priceTickSize: string;
  /** Quantity tick size */
  quantityTickSize: string;
  /** Maker fee rate */
  makerFee: string;
  /** Taker fee rate */
  takerFee: string;
  /** Market status */
  status: MarketStatus;
  /** Base token decimals */
  baseDecimals: number;
  /** Quote token decimals */
  quoteDecimals: number;
}

/**
 * Market status enumeration
 */
export enum MarketStatus {
  /** Market is active and trading */
  Active = "active",
  /** Market is paused */
  Paused = "paused",
  /** Market is closed */
  Closed = "closed",
  /** Market is in pre-launch */
  PreLaunch = "pre_launch",
}

/**
 * Position information
 */
export interface Position {
  /** Market ID */
  marketId: string;
  /** Market symbol */
  symbol: string;
  /** Position side (long/short) */
  side: "long" | "short";
  /** Position size */
  size: string;
  /** Entry price */
  entryPrice: string;
  /** Mark price */
  markPrice: string;
  /** Liquidation price */
  liquidationPrice: string;
  /** Unrealized PnL */
  unrealizedPnl: string;
  /** Realized PnL */
  realizedPnl: string;
  /** Margin */
  margin: string;
  /** Leverage */
  leverage: number;
  /** Position value */
  notional: string;
  /** Position creation timestamp */
  createdAt: number;
  /** Last update timestamp */
  updatedAt: number;
}

/**
 * Ticker information
 */
export interface Ticker {
  /** Market symbol */
  symbol: string;
  /** Last traded price */
  lastPrice: string;
  /** 24h price change */
  priceChange: string;
  /** 24h price change percentage */
  priceChangePercent: string;
  /** 24h high price */
  high24h: string;
  /** 24h low price */
  low24h: string;
  /** 24h volume (base token) */
  volume24h: string;
  /** 24h quote volume */
  quoteVolume24h: string;
  /** Best bid price */
  bidPrice: string;
  /** Best bid quantity */
  bidQuantity: string;
  /** Best ask price */
  askPrice: string;
  /** Best ask quantity */
  askQuantity: string;
  /** Timestamp */
  timestamp: number;
}

/**
 * Trade information
 */
export interface Trade {
  /** Trade ID */
  tradeId: string;
  /** Market symbol */
  symbol: string;
  /** Trade price */
  price: string;
  /** Trade quantity */
  quantity: string;
  /** Quote quantity */
  quoteQuantity: string;
  /** Trade side (buy/sell from taker perspective) */
  side: "buy" | "sell";
  /** Trade timestamp */
  timestamp: number;
  /** Is buyer maker */
  isBuyerMaker: boolean;
}

/**
 * Order book level
 */
export interface OrderBookLevel {
  /** Price level */
  price: string;
  /** Quantity at this level */
  quantity: string;
}

/**
 * Order book snapshot
 */
export interface OrderBook {
  /** Market symbol */
  symbol: string;
  /** Bid levels */
  bids: OrderBookLevel[];
  /** Ask levels */
  asks: OrderBookLevel[];
  /** Last update ID */
  lastUpdateId: number;
  /** Timestamp */
  timestamp: number;
}

/**
 * Pagination parameters
 */
export interface PaginationParams {
  /** Page number (starting from 1) */
  page?: number;
  /** Page size (number of items per page) */
  pageSize?: number;
  /** Start time for filtering (unix timestamp) */
  startTime?: number;
  /** End time for filtering (unix timestamp) */
  endTime?: number;
}

/**
 * Paginated response wrapper
 * @template T - The type of items in the paginated list
 */
export interface PaginatedResponse<T> {
  /** List of items */
  items: T[];
  /** Total number of items */
  total: number;
  /** Current page number */
  page: number;
  /** Page size */
  pageSize: number;
  /** Total number of pages */
  totalPages: number;
  /** Has next page */
  hasNext: boolean;
  /** Has previous page */
  hasPrev: boolean;
}
