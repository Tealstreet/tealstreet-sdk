/**
 * Transaction types and interfaces for the Lighter SDK
 *
 * This file contains all L2 transaction types and related structures
 * for interacting with the Lighter protocol.
 */

import { OrderType, TimeInForce, OrderSide, TxTypeValue } from './constants';

/**
 * Transaction options common to all L2 transactions
 */
export interface TransactOpts {
  /** Transaction nonce (must be sequential) */
  nonce: number;
  /** Transaction expiry timestamp (unix seconds) */
  expiry: number;
  /** Account index (0-255) */
  accountIndex: number;
  /** API key index (0-255) */
  apiKeyIndex: number;
  /** Gas price (optional, will use default if not provided) */
  gasPrice?: string;
  /** Gas limit (optional, will use default if not provided) */
  gasLimit?: number;
}

/**
 * L2 Create Order transaction information
 */
export interface L2CreateOrderTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2CreateOrder;
  /** Market ID */
  marketId: string;
  /** Order type */
  orderType: OrderType;
  /** Order side (0 = buy, 1 = sell) */
  side: OrderSide;
  /** Order price (in wei or smallest unit) */
  price: string;
  /** Order quantity (in wei or smallest unit) */
  quantity: string;
  /** Time in force */
  timeInForce: TimeInForce;
  /** Reduce only flag */
  reduceOnly: boolean;
  /** Post only flag */
  postOnly: boolean;
  /** Stop price (for stop orders, in wei) */
  stopPrice?: string;
  /** Client order ID */
  clientOrderId?: string;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Cancel Order transaction information
 */
export interface L2CancelOrderTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2CancelOrder;
  /** Market ID */
  marketId: string;
  /** Order ID to cancel */
  orderId: string;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Transfer transaction information
 */
export interface L2TransferTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2Transfer;
  /** Token address */
  tokenAddress: string;
  /** Recipient address */
  recipient: string;
  /** Transfer amount (in wei or smallest unit) */
  amount: string;
  /** Memo/note for the transfer */
  memo?: string;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Withdraw transaction information
 */
export interface L2WithdrawTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2Withdraw;
  /** Token address */
  tokenAddress: string;
  /** Withdrawal amount (in wei or smallest unit) */
  amount: string;
  /** L1 recipient address (optional, defaults to sender) */
  l1Recipient?: string;
  /** Fast withdrawal flag (higher fee, faster processing) */
  fastWithdraw?: boolean;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Update API Key transaction information
 */
export interface L2UpdateApiKeyTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2UpdateApiKey;
  /** API key index to update */
  apiKeyIndex: number;
  /** New API key public key */
  newPublicKey: string;
  /** Permissions bitmap */
  permissions: number;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Liquidation transaction information
 */
export interface L2LiquidationTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2Liquidation;
  /** Account to liquidate */
  liquidateeAccount: string;
  /** Market ID */
  marketId: string;
  /** Liquidation quantity */
  quantity: string;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Deleverage transaction information
 */
export interface L2DeleverageTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2Deleverage;
  /** Account to deleverage */
  deleverageeAccount: string;
  /** Market ID */
  marketId: string;
  /** Deleverage quantity */
  quantity: string;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * L2 Funding Fee transaction information
 */
export interface L2FundingFeeTxInfo {
  /** Transaction type */
  txType: typeof import('./constants').TxType.L2FundingFee;
  /** Market ID */
  marketId: string;
  /** Funding rate */
  fundingRate: string;
  /** Transaction options */
  opts: TransactOpts;
}

/**
 * Union type of all L2 transaction info types
 */
export type L2TxInfo =
  | L2CreateOrderTxInfo
  | L2CancelOrderTxInfo
  | L2TransferTxInfo
  | L2WithdrawTxInfo
  | L2UpdateApiKeyTxInfo
  | L2LiquidationTxInfo
  | L2DeleverageTxInfo
  | L2FundingFeeTxInfo;

/**
 * Create Order transaction request
 */
export interface CreateOrderTxReq {
  /** Market symbol */
  symbol: string;
  /** Order type */
  type: OrderType;
  /** Order side */
  side: OrderSide;
  /** Order price */
  price: string;
  /** Order quantity */
  quantity: string;
  /** Time in force */
  timeInForce?: TimeInForce;
  /** Reduce only */
  reduceOnly?: boolean;
  /** Post only */
  postOnly?: boolean;
  /** Stop price (for stop orders) */
  stopPrice?: string;
  /** Client order ID */
  clientOrderId?: string;
  /** Nonce */
  nonce?: number;
  /** Expiry timestamp */
  expiry?: number;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Cancel Order transaction request
 */
export interface CancelOrderTxReq {
  /** Market symbol */
  symbol: string;
  /** Order ID */
  orderId: string;
  /** Nonce */
  nonce?: number;
  /** Expiry timestamp */
  expiry?: number;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Transfer transaction request
 */
export interface TransferTxReq {
  /** Token symbol or address */
  token: string;
  /** Recipient address */
  recipient: string;
  /** Transfer amount */
  amount: string;
  /** Memo */
  memo?: string;
  /** Nonce */
  nonce?: number;
  /** Expiry timestamp */
  expiry?: number;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Withdraw transaction request
 */
export interface WithdrawTxReq {
  /** Token symbol or address */
  token: string;
  /** Withdrawal amount */
  amount: string;
  /** L1 recipient address */
  l1Recipient?: string;
  /** Fast withdraw flag */
  fastWithdraw?: boolean;
  /** Nonce */
  nonce?: number;
  /** Expiry timestamp */
  expiry?: number;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Update API Key transaction request
 */
export interface UpdateApiKeyTxReq {
  /** API key index */
  apiKeyIndex: number;
  /** New public key */
  newPublicKey: string;
  /** Permissions bitmap */
  permissions: number;
  /** Nonce */
  nonce?: number;
  /** Expiry timestamp */
  expiry?: number;
  /** Account index */
  accountIndex?: number;
}

/**
 * Transaction signature
 */
export interface TxSignature {
  /** R component of the signature */
  r: string;
  /** S component of the signature */
  s: string;
  /** V component of the signature */
  v: number;
}

/**
 * Signed transaction
 */
export interface SignedTx {
  /** Transaction information */
  tx: L2TxInfo;
  /** Transaction signature */
  signature: TxSignature;
  /** Signer address */
  signer: string;
}

/**
 * Transaction receipt
 */
export interface TxReceipt {
  /** Transaction hash */
  txHash: string;
  /** Transaction type */
  txType: TxTypeValue;
  /** Transaction status (0 = pending, 1 = success, 2 = failed) */
  status: number;
  /** Block number */
  blockNumber?: number;
  /** Block timestamp */
  blockTimestamp?: number;
  /** Gas used */
  gasUsed?: string;
  /** Error message (if failed) */
  error?: string;
  /** Transaction data */
  data?: any;
}

/**
 * Batch transaction request
 */
export interface BatchTxReq {
  /** List of transactions */
  transactions: (CreateOrderTxReq | CancelOrderTxReq | TransferTxReq | WithdrawTxReq)[];
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Batch transaction response
 */
export interface BatchTxResponse {
  /** List of transaction hashes */
  txHashes: string[];
  /** List of successful transaction indices */
  successIndices: number[];
  /** List of failed transaction indices */
  failedIndices: number[];
  /** Error messages for failed transactions */
  errors: Record<number, string>;
}
