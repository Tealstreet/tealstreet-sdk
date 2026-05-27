/**
 * Lighter SDK
 *
 * Complete TypeScript SDK for interacting with the Lighter protocol.
 *
 * ## Features
 *
 * - **Type-safe API client** - Fully typed HTTP API client with request/response interceptors
 * - **Transaction signing** - Schnorr signature support using Poseidon2 hash and ECgFp5 curve
 * - **Key management** - Secure key generation and management utilities
 * - **Order management** - Create, cancel, and query orders with full type safety
 * - **Account management** - Manage accounts, balances, and positions
 * - **Market data** - Access real-time and historical market data
 * - **WebSocket support** - Subscribe to real-time updates (order books, trades, tickers)
 *
 * ## Quick Start
 *
 * ```typescript
 * import { LighterApiClient, TransactionClient } from '@tealstreet/lighter-sdk';
 * import { createSigner, generateKeyPair } from '@tealstreet/lighter-sdk/signer';
 * import { OrderSide } from '@tealstreet/lighter-sdk/types';
 *
 * // Initialize API client
 * const apiClient = new LighterApiClient({
 *   baseUrl: 'https://api.lighter.xyz',
 *   timeout: 30000
 * });
 *
 * // Generate key pair
 * const keys = await generateKeyPair();
 *
 * // Initialize signer
 * const signer = await createSigner();
 *
 * // Create transaction client
 * const txClient = new TransactionClient({
 *   httpClient: apiClient,
 *   crypto: signer,
 *   chainId: 1,
 *   privateKey: keys.privateKey,
 *   accountIndex: 0n,
 *   apiKeyIndex: 0
 * });
 *
 * // Create an order
 * const orderTx = await txClient.getCreateOrderTransaction({
 *   orderId: 123n,
 *   marketId: 1,
 *   isBuy: true,
 *   price: 1000000n,
 *   amount: 500000n
 * });
 *
 * // Submit transaction
 * const txHash = await apiClient.sendRawTx(orderTx);
 * console.log('Transaction hash:', txHash);
 * ```
 *
 * ## Installation
 *
 * ```bash
 * npm install @tealstreet/lighter-sdk
 * # or
 * yarn add @tealstreet/lighter-sdk
 * ```
 *
 * ## Documentation
 *
 * For detailed documentation, see the README.md file or visit the official documentation.
 *
 * @module @tealstreet/lighter-sdk
 * @packageDocumentation
 */

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * All type definitions, enums, and interfaces
 * @see {@link module:types}
 */
export * from './types';

// ============================================================================
// Client Classes
// ============================================================================

/**
 * HTTP API client and specialized API clients
 * @see {@link LighterApiClient}
 * @see {@link TransactionClient}
 * @see {@link AccountApi}
 * @see {@link OrderApi}
 * @see {@link TransactionApi}
 * @see {@link CandlestickApi}
 * @see {@link FundingApi}
 * @see {@link InfoApi}
 * @see {@link ReferralApi}
 */
export {
  // Core clients
  LighterApiClient,
  TransactionClient,
  LighterApiError,
  type LighterApiClientConfig,
  type Interceptors,
  type RequestInterceptor,
  type ResponseInterceptor,
  type ErrorInterceptor,
  type ApiKey,
  type GetApiKeysResponse,
  type GetNonceResponse,
  type GetTransferFeeResponse,
  type TransferFeeInfo,
  type SendTxResponse,
  type ApiErrorResponse,
  type ApiResponse,
  type L2ApproveIntegratorTxInfo,
  type L2CancelOrderTxInfo,
  type L2CreateGroupedOrdersTxInfo,
  type L2CreateOrderTxInfo,
  type L2TransferTxInfo,
  type LighterGroupedOrderDetails,
  type SendTxBatchResponse,
  type TxInfo,
  type ApproveIntegratorParams,
  type CancelOrderParams,
  type CreateGroupedOrdersParams,
  type CreateOrderParams,
  type LighterGroupedOrderType,
  type TransferParams,
  // Account API
  AccountApi,
  type AccountApiKeys,
  type AccountInfo,
  type AccountLimits,
  type AccountMetadata,
  type AccountsByL1Address,
  type ApiKeyInfo,
  type GetLiquidationsOptions,
  type GetPnlOptions,
  type Liquidation,
  type Liquidations,
  type PnL,
  type PnLDataPoint,
  type SubAccount,
  // Order API
  OrderApi,
  type AccountOrdersResponse,
  type ApiOrder,
  type ApiTrade,
  type ExchangeStats,
  type ExchangeStatsResponse,
  type GetInactiveOrdersOptions,
  type GetTradesOptions,
  type OrderBookDetailsResponse,
  type OrderBookOrdersResponse,
  type OrderBooksResponse,
  type RecentTradesResponse,
  type TradesResponse,
  // Transaction API
  TransactionApi,
  TransactionStatus,
  TransactionType,
  type BlockTransactionsResponse,
  type Deposit,
  type DepositHistoryResponse,
  type GetAccountTransactionsOptions,
  type GetDepositHistoryOptions,
  type GetTransferHistoryOptions,
  type Transaction,
  type TransactionDetailResponse,
  type TransactionsResponse,
  type Transfer,
  type TransferHistoryResponse,
  // Candlestick API
  CandlestickApi,
  type Candlestick,
  type CandlesticksResponse,
  type Funding,
  type FundingsResponse,
  // Funding API
  FundingApi,
  type FundingRate,
  type FundingRatesResponse,
  // Info API
  InfoApi,
  type WithdrawalDelay,
  type WithdrawalDelayResponse,
  // Referral API
  ReferralApi,
  type ReferralPoints,
  type ReferralPointsResponse,
} from './client';

// ============================================================================
// Signer Module
// ============================================================================

/**
 * Transaction signing, key management, and cryptographic utilities
 * @see {@link module:signer}
 */
export * from './signer';

// ============================================================================
// Utilities
// ============================================================================

/**
 * Common utility functions
 * @see {@link module:utils}
 */
export {
  bigIntToHex,
  hexToBigInt,
  validateAddress,
  formatAmount,
  parseAmount,
  sleep,
  retry,
  clampBigInt,
  isInRange,
  numberToBigInt,
  percentageOf,
  truncate,
  formatAddress,
  isValidAccountIndex,
  isValidApiKeyIndex,
  isValidNonce,
  type RetryOptions,
} from './utils';

/**
 * Validation utilities
 * @see {@link module:utils/validation}
 */
export {
  ValidationError,
  validateAccountIndex,
  validateApiKeyIndex,
  validateNonce,
  validateOrderId,
  validateMarketId,
  validatePrice,
  validateAmount,
  validateOrderParams,
  validatePrivateKey,
  validatePublicKey,
  validateDeadline,
  validateChainId,
} from './utils/validation';

// ============================================================================
// Constants
// ============================================================================

/**
 * Constants for chain IDs, API endpoints, defaults, and error codes
 * @see {@link module:constants}
 */
export {
  ChainId,
  ApiEndpoints,
  WsEndpoints,
  Defaults,
  RateLimits,
  HttpStatus,
  ErrorCode,
  // Note: TransactionType is exported from client/TransactionApi instead of constants
  // to avoid conflicts and provide more specific transaction types for API responses
  TokenDecimals,
  FeePrecision,
  MaxSlippageBps,
  MinSlippageBps,
  DefaultSlippageBps,
  BasisPoints,
  ProtocolVersion,
  SdkVersion,
  UserAgent,
  WsProtocolVersion,
  MaxWsMessageSize,
  OrderBookDepth,
  KlineInterval,
  ZeroAddress,
  MaxUint256,
  MaxUint128,
  MaxUint64,
  MaxUint32,
  MaxUint16,
  MaxUint8,
  EnvVars,
  type ChainIdValue,
  type ErrorCodeValue,
  type KlineIntervalValue,
} from './constants';

// ============================================================================
// Default Export
// ============================================================================

import { LighterApiClient } from './client/api-client';
import { TransactionClient } from './client/transaction-client';
import { TransactionSigner, createSigner } from './signer/signer';
import { generateKeyPair, derivePublicKey } from './signer/key-manager';
import { SdkVersion, ProtocolVersion } from './constants';

/**
 * Default export containing the main SDK classes and utilities
 *
 * @example
 * ```typescript
 * import LighterSDK from '@tealstreet/lighter-sdk';
 *
 * // Access version info
 * console.log('SDK Version:', LighterSDK.version);
 * console.log('Protocol Version:', LighterSDK.protocolVersion);
 *
 * // Create API client
 * const client = new LighterSDK.ApiClient({ baseUrl: '...' });
 *
 * // Generate keys
 * const keys = await LighterSDK.generateKeyPair();
 *
 * // Create signer
 * const signer = await LighterSDK.createSigner();
 * ```
 */
const LighterSDK = {
  /**
   * SDK version
   */
  version: SdkVersion,

  /**
   * Protocol version
   */
  protocolVersion: ProtocolVersion,

  /**
   * HTTP API client class
   */
  ApiClient: LighterApiClient,

  /**
   * Transaction client class
   */
  TransactionClient: TransactionClient,

  /**
   * Transaction signer class
   */
  Signer: TransactionSigner,

  /**
   * Create a new signer instance
   */
  createSigner,

  /**
   * Generate a new key pair
   */
  generateKeyPair,

  /**
   * Derive public key from private key
   */
  derivePublicKey,
};

export default LighterSDK;
