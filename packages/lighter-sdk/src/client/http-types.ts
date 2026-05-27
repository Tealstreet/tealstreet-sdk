/**
 * HTTP Client Type Definitions for Lighter SDK
 */

/**
 * Configuration for the Lighter API client
 */
export interface LighterApiClientConfig {
  /** Base URL for the Lighter API */
  baseUrl: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Additional HTTP headers to include in all requests */
  headers?: Record<string, string>;
  /** Enable request/response logging for debugging */
  debug?: boolean;
}

/**
 * API key information returned from the server
 */
export interface ApiKey {
  /** Index of this API key */
  index: number;
  /** Public key associated with this API key (hex string) */
  publicKey: string;
  /** Whether this API key is active */
  active: boolean;
  /** Creation timestamp */
  createdAt?: number;
  /** Last used timestamp */
  lastUsedAt?: number;
}

/**
 * Transfer fee information
 */
export interface TransferFeeInfo {
  /** Fee amount in base units */
  fee: bigint;
  /** Minimum transfer amount */
  minAmount: bigint;
  /** Maximum transfer amount */
  maxAmount: bigint;
  /** Fee percentage (basis points) */
  feePercentage?: number;
}

/**
 * Transaction information for Layer 2 operations
 */
export interface TxInfo {
  /** Transaction type */
  type:
    | 'create_order'
    | 'cancel_order'
    | 'transfer'
    | 'withdrawal'
    | 'update_leverage'
    | 'approve_integrator'
    | 'create_grouped_orders';
  /** Serialized transaction data (hex string) */
  data: string;
  /** Transaction signature (hex string) */
  signature: string;
  /** Account index */
  accountIndex: bigint;
  /** API key index used for signing */
  apiKeyIndex: number;
  /** Transaction nonce */
  nonce: bigint;
  /** Request expiry timestamp in milliseconds */
  expiredAt?: bigint;
}

/**
 * Create order transaction info (L2)
 */
export interface L2CreateOrderTxInfo extends TxInfo {
  type: 'create_order';
  /** Order details */
  orderDetails: {
    orderId: bigint;
    marketId: number;
    isBuy: boolean;
    price: bigint;
    amount: bigint;
    postOnly?: boolean;
    reduceOnly?: boolean;
    orderTypeValue?: number;
    timeInForceValue?: number;
    triggerPrice?: bigint;
    orderExpiry?: bigint;
    integratorAccountIndex?: bigint;
    integratorMakerFee?: number;
    integratorTakerFee?: number;
  };
}

export interface LighterGroupedOrderDetails {
  orderId: bigint;
  marketId: number;
  isBuy: boolean;
  price: bigint;
  amount: bigint;
  reduceOnly?: boolean;
  orderTypeValue?: number;
  timeInForceValue?: number;
  triggerPrice?: bigint;
  orderExpiry?: bigint;
}

/**
 * Create grouped orders transaction info (L2).
 */
export interface L2CreateGroupedOrdersTxInfo extends TxInfo {
  type: 'create_grouped_orders';
  groupedOrderDetails: {
    groupingType: 1 | 2 | 3;
    orders: LighterGroupedOrderDetails[];
    integratorAccountIndex?: bigint;
    integratorMakerFee?: number;
    integratorTakerFee?: number;
  };
}

/**
 * Cancel order transaction info (L2)
 */
export interface L2CancelOrderTxInfo extends TxInfo {
  type: 'cancel_order';
  /** Cancel order details */
  cancelDetails: {
    orderId: bigint;
    marketId: number;
    expiredAt?: bigint;
  };
}

/**
 * Transfer transaction info (L2)
 */
export interface L2TransferTxInfo extends TxInfo {
  type: 'transfer';
  /** Transfer details */
  transferDetails: {
    toAccountIndex: bigint;
    amount: bigint;
    tokenId: number;
  };
}

/**
 * Update leverage transaction info (L2)
 */
export interface L2UpdateLeverageTxInfo extends TxInfo {
  type: 'update_leverage';
  /** Update leverage details */
  leverageDetails: {
    marketId: number;
    initialMarginFraction: number;
    marginMode: 0 | 1;
    expiredAt?: bigint;
  };
}

/**
 * Approve integrator transaction info (L2)
 */
export interface L2ApproveIntegratorTxInfo extends TxInfo {
  type: 'approve_integrator';
  /** Approve integrator details */
  approveIntegratorDetails: {
    integratorAccountIndex: bigint;
    maxPerpsTakerFee: number;
    maxPerpsMakerFee: number;
    maxSpotTakerFee: number;
    maxSpotMakerFee: number;
    approvalExpiry: bigint;
    expiredAt?: bigint;
    /** Optional L1 signature for cross-master approval path */
    l1Signature?: string;
  };
}

/**
 * Withdrawal transaction info (L2). Carries no extra details beyond the base
 * fields — the discriminant is the only thing that narrows it.
 */
export interface L2WithdrawalTxInfo extends TxInfo {
  type: 'withdrawal';
}

/**
 * Any L2 transaction info — a true discriminated union (on `type`) of every
 * concrete transaction shape accepted by `sendRawTx` / `sendRawTxBatch`. Each
 * member extends {@link TxInfo}; we list them explicitly rather than including
 * the base `TxInfo`, which would collapse the union back to the base type and
 * defeat per-variant property checking.
 */
export type AnyTxInfo =
  | L2CreateOrderTxInfo
  | L2CreateGroupedOrdersTxInfo
  | L2CancelOrderTxInfo
  | L2TransferTxInfo
  | L2UpdateLeverageTxInfo
  | L2ApproveIntegratorTxInfo
  | L2WithdrawalTxInfo;

/**
 * API error response
 */
export interface ApiErrorResponse {
  /** Error code */
  code: string;
  /** Human-readable error message */
  message: string;
  /** Additional error details */
  details?: Record<string, any>;
  /** Request ID for debugging */
  requestId?: string;
}

/**
 * API success response wrapper
 */
export interface ApiResponse<T> {
  /** Whether the request was successful */
  success: boolean;
  /** Response data */
  data?: T;
  /** Error information if success is false */
  error?: ApiErrorResponse;
  /** Request timestamp */
  timestamp?: number;
}

/**
 * Get nonce response
 */
export interface GetNonceResponse {
  /** Current nonce for the account/API key combination */
  nonce: string; // BigInt as string
  /** Next available nonce */
  nextNonce: string; // BigInt as string
}

/**
 * Get API keys response
 */
export interface GetApiKeysResponse {
  /** List of API keys for the account */
  apiKeys: ApiKey[];
}

/**
 * Send transaction response
 */
export interface SendTxResponse {
  /** Transaction hash */
  txHash: string;
  /** Transaction status */
  status: 'pending' | 'confirmed' | 'failed';
  /** Block number (if confirmed) */
  blockNumber?: number;
}

export interface SendTxBatchResponse {
  txHashes: string[];
  predictedExecutionTimeMs?: number;
  volumeQuotaRemaining?: number | string;
}

/**
 * Get transfer fee response
 */
export interface GetTransferFeeResponse {
  /** Fee amount as string (BigInt) */
  fee: string;
  /** Minimum transfer amount as string (BigInt) */
  minAmount: string;
  /** Maximum transfer amount as string (BigInt) */
  maxAmount: string;
  /** Fee percentage in basis points */
  feePercentage?: number;
}

/**
 * HTTP request interceptor function
 */
export type RequestInterceptor = (config: any) => any | Promise<any>;

/**
 * HTTP response interceptor function
 */
export type ResponseInterceptor = (response: any) => any | Promise<any>;

/**
 * HTTP error interceptor function
 */
export type ErrorInterceptor = (error: any) => any;

/**
 * Interceptor configuration
 */
export interface Interceptors {
  /** Request interceptors */
  request?: RequestInterceptor[];
  /** Response interceptors */
  response?: ResponseInterceptor[];
  /** Error interceptors */
  error?: ErrorInterceptor[];
}

/**
 * Custom error class for Lighter API errors
 */
export class LighterApiError extends Error {
  public readonly code: string;
  public readonly details?: Record<string, any>;
  public readonly requestId?: string;
  public readonly statusCode?: number;

  constructor(
    message: string,
    code: string = 'UNKNOWN_ERROR',
    details?: Record<string, any>,
    requestId?: string,
    statusCode?: number,
  ) {
    super(message);
    this.name = 'LighterApiError';
    this.code = code;
    this.details = details;
    this.requestId = requestId;
    this.statusCode = statusCode;

    // Maintains proper stack trace for where our error was thrown (only available on V8)
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, LighterApiError);
    }
  }
}
