/**
 * Lighter SDK Constants
 *
 * Additional constants for chain configuration, API endpoints, and default values.
 */

// Re-export all constants from types
export * from '../types/constants';

/**
 * Chain IDs for different networks
 */
export const ChainId = {
  /** Ethereum Mainnet */
  Mainnet: 1,
  /** Ethereum Sepolia Testnet */
  Sepolia: 11155111,
  /** Arbitrum One */
  ArbitrumOne: 42161,
  /** Arbitrum Sepolia Testnet */
  ArbitrumSepolia: 421614,
  /** Optimism */
  Optimism: 10,
  /** Optimism Sepolia Testnet */
  OptimismSepolia: 11155420,
  /** Polygon */
  Polygon: 137,
  /** Polygon Mumbai Testnet (deprecated) */
  PolygonMumbai: 80001,
  /** Base */
  Base: 8453,
  /** Base Sepolia Testnet */
  BaseSepolia: 84532,
} as const;

export type ChainIdValue = typeof ChainId[keyof typeof ChainId];

/**
 * Default API endpoints for different environments
 */
export const ApiEndpoints = {
  /** Production API endpoint */
  Production: 'https://api.lighter.xyz',
  /** Staging API endpoint */
  Staging: 'https://api-staging.lighter.xyz',
  /** Testnet API endpoint */
  Testnet: 'https://api-testnet.lighter.xyz',
  /** Development API endpoint */
  Development: 'http://localhost:8080',
} as const;

/**
 * Default WebSocket endpoints for different environments
 */
export const WsEndpoints = {
  /** Production WebSocket endpoint */
  Production: 'wss://ws.lighter.xyz',
  /** Staging WebSocket endpoint */
  Staging: 'wss://ws-staging.lighter.xyz',
  /** Testnet WebSocket endpoint */
  Testnet: 'wss://ws-testnet.lighter.xyz',
  /** Development WebSocket endpoint */
  Development: 'ws://localhost:8081',
} as const;

/**
 * Default configuration values
 */
export const Defaults = {
  /** Default request timeout in milliseconds */
  RequestTimeout: 30000,
  /** Default WebSocket ping interval in milliseconds */
  WsPingInterval: 30000,
  /** Default WebSocket reconnect delay in milliseconds */
  WsReconnectDelay: 5000,
  /** Default maximum WebSocket reconnect attempts */
  WsMaxReconnectAttempts: 10,
  /** Default maximum retry attempts for API requests */
  MaxRetryAttempts: 3,
  /** Default retry delay in milliseconds */
  RetryDelay: 1000,
  /** Default page size for paginated requests */
  PageSize: 50,
  /** Default maximum page size */
  MaxPageSize: 1000,
  /** Default auth token validity in seconds (1 hour) */
  AuthTokenValidity: 3600,
  /** Default order expiry in seconds (24 hours) */
  OrderExpiry: 86400,
  /** Default minimum auth token future time in seconds */
  MinAuthTokenFuture: 60,
} as const;

/**
 * Rate limit tiers
 */
export const RateLimits = {
  /** Public endpoint rate limit (requests per minute) */
  Public: 60,
  /** Authenticated endpoint rate limit (requests per minute) */
  Authenticated: 300,
  /** Trading endpoint rate limit (requests per minute) */
  Trading: 600,
  /** WebSocket message rate limit (messages per second) */
  WebSocket: 100,
} as const;

/**
 * HTTP status codes commonly used by the API
 */
export const HttpStatus = {
  /** Success */
  OK: 200,
  /** Resource created */
  Created: 201,
  /** Request accepted */
  Accepted: 202,
  /** No content */
  NoContent: 204,
  /** Bad request */
  BadRequest: 400,
  /** Unauthorized */
  Unauthorized: 401,
  /** Forbidden */
  Forbidden: 403,
  /** Not found */
  NotFound: 404,
  /** Conflict */
  Conflict: 409,
  /** Rate limit exceeded */
  TooManyRequests: 429,
  /** Internal server error */
  InternalServerError: 500,
  /** Bad gateway */
  BadGateway: 502,
  /** Service unavailable */
  ServiceUnavailable: 503,
  /** Gateway timeout */
  GatewayTimeout: 504,
} as const;

/**
 * Error codes for the SDK
 */
export const ErrorCode = {
  /** Network error */
  NetworkError: 'NETWORK_ERROR',
  /** Timeout error */
  Timeout: 'TIMEOUT',
  /** Invalid parameter */
  InvalidParameter: 'INVALID_PARAMETER',
  /** Invalid signature */
  InvalidSignature: 'INVALID_SIGNATURE',
  /** Insufficient balance */
  InsufficientBalance: 'INSUFFICIENT_BALANCE',
  /** Order not found */
  OrderNotFound: 'ORDER_NOT_FOUND',
  /** Market not found */
  MarketNotFound: 'MARKET_NOT_FOUND',
  /** Nonce too low */
  NonceTooLow: 'NONCE_TOO_LOW',
  /** Nonce too high */
  NonceTooHigh: 'NONCE_TOO_HIGH',
  /** Transaction expired */
  TransactionExpired: 'TX_EXPIRED',
  /** Rate limit exceeded */
  RateLimitExceeded: 'RATE_LIMIT_EXCEEDED',
  /** Authentication failed */
  AuthenticationFailed: 'AUTH_FAILED',
  /** Crypto not initialized */
  CryptoNotInitialized: 'CRYPTO_NOT_INITIALIZED',
  /** Internal error */
  InternalError: 'INTERNAL_ERROR',
  /** Unknown error */
  Unknown: 'UNKNOWN',
} as const;

export type ErrorCodeValue = typeof ErrorCode[keyof typeof ErrorCode];

/**
 * Transaction types (mapped to backend constants)
 */
export const TransactionType = {
  /** Create order transaction */
  CreateOrder: 'create_order',
  /** Cancel order transaction */
  CancelOrder: 'cancel_order',
  /** Transfer transaction */
  Transfer: 'transfer',
  /** Withdraw transaction */
  Withdraw: 'withdraw',
  /** Update API key transaction */
  UpdateApiKey: 'update_api_key',
} as const;

export type TransactionTypeValue = typeof TransactionType[keyof typeof TransactionType];

/**
 * Token decimals for common tokens
 */
export const TokenDecimals = {
  /** USDT decimals */
  USDT: 6,
  /** USDC decimals */
  USDC: 6,
  /** DAI decimals */
  DAI: 18,
  /** ETH/WETH decimals */
  ETH: 18,
  /** BTC/WBTC decimals */
  BTC: 8,
} as const;

/**
 * Fee precision (number of decimal places for fees)
 */
export const FeePrecision = 6;

/**
 * Maximum slippage tolerance (in basis points, 100 = 1%)
 */
export const MaxSlippageBps = 10000; // 100%

/**
 * Minimum slippage tolerance (in basis points)
 */
export const MinSlippageBps = 1; // 0.01%

/**
 * Default slippage tolerance (in basis points, 50 = 0.5%)
 */
export const DefaultSlippageBps = 50;

/**
 * Basis points divisor (10000 basis points = 100%)
 */
export const BasisPoints = 10000;

/**
 * Protocol version
 */
export const ProtocolVersion = '1.0.0';

/**
 * SDK version (should match package.json version)
 */
export const SdkVersion = '1.0.0';

/**
 * User agent string for HTTP requests
 */
export const UserAgent = `lighter-sdk/${SdkVersion}`;

/**
 * WebSocket protocol version
 */
export const WsProtocolVersion = '1.0';

/**
 * Maximum WebSocket message size in bytes
 */
export const MaxWsMessageSize = 1024 * 1024; // 1 MB

/**
 * Order book depth levels
 */
export const OrderBookDepth = {
  /** Shallow depth (top 10 levels) */
  Shallow: 10,
  /** Medium depth (top 50 levels) */
  Medium: 50,
  /** Deep depth (top 100 levels) */
  Deep: 100,
  /** Full depth (all levels) */
  Full: 1000,
} as const;

/**
 * Kline/Candlestick intervals
 */
export const KlineInterval = {
  /** 1 minute */
  OneMinute: '1m',
  /** 5 minutes */
  FiveMinutes: '5m',
  /** 15 minutes */
  FifteenMinutes: '15m',
  /** 30 minutes */
  ThirtyMinutes: '30m',
  /** 1 hour */
  OneHour: '1h',
  /** 4 hours */
  FourHours: '4h',
  /** 1 day */
  OneDay: '1d',
  /** 1 week */
  OneWeek: '1w',
  /** 1 month */
  OneMonth: '1M',
} as const;

export type KlineIntervalValue = typeof KlineInterval[keyof typeof KlineInterval];

/**
 * Zero address constant
 */
export const ZeroAddress = '0x0000000000000000000000000000000000000000';

/**
 * Maximum uint256 value
 */
export const MaxUint256 = BigInt('0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff');

/**
 * Maximum uint128 value
 */
export const MaxUint128 = BigInt('0xffffffffffffffffffffffffffffffff');

/**
 * Maximum uint64 value
 */
export const MaxUint64 = BigInt('0xffffffffffffffff');

/**
 * Maximum uint32 value
 */
export const MaxUint32 = 0xffffffff;

/**
 * Maximum uint16 value
 */
export const MaxUint16 = 0xffff;

/**
 * Maximum uint8 value
 */
export const MaxUint8 = 0xff;

/**
 * Environment variable names
 */
export const EnvVars = {
  /** API base URL */
  ApiUrl: 'LIGHTER_API_URL',
  /** WebSocket URL */
  WsUrl: 'LIGHTER_WS_URL',
  /** Chain ID */
  ChainId: 'LIGHTER_CHAIN_ID',
  /** Private key */
  PrivateKey: 'LIGHTER_PRIVATE_KEY',
  /** Account index */
  AccountIndex: 'LIGHTER_ACCOUNT_INDEX',
  /** API key index */
  ApiKeyIndex: 'LIGHTER_API_KEY_INDEX',
  /** Debug mode */
  Debug: 'LIGHTER_DEBUG',
} as const;
