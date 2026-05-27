/**
 * API response types for the Lighter SDK
 *
 * This file contains all response types returned by the Lighter API.
 */

import { ResultCode } from './constants';

/**
 * Result code wrapper for responses
 */
export interface Result {
  /** Result code */
  code: ResultCode;
  /** Result message */
  message: string;
}

/**
 * Next nonce response
 */
export interface NextNonce {
  /** Account address */
  account: string;
  /** Next available nonce */
  nonce: number;
  /** Current pending nonce */
  pendingNonce: number;
  /** Last confirmed nonce */
  confirmedNonce: number;
}

/**
 * Account API key information
 */
export interface ApiKeyInfo {
  /** API key index */
  index: number;
  /** Public key */
  publicKey: string;
  /** Permissions bitmap */
  permissions: number;
  /** Key label/name */
  label?: string;
  /** Is active */
  isActive: boolean;
  /** Created at timestamp */
  createdAt: number;
  /** Last used timestamp */
  lastUsedAt?: number;
}

/**
 * Account API keys response
 */
export interface AccountApiKeys {
  /** Account address */
  account: string;
  /** List of API keys */
  apiKeys: ApiKeyInfo[];
}

/**
 * Transaction hash response
 */
export interface TxHash {
  /** Transaction hash */
  txHash: string;
  /** Transaction nonce */
  nonce: number;
  /** Submitted at timestamp */
  submittedAt: number;
}

/**
 * Error response
 */
export interface ErrorResponse {
  /** Error code */
  code: ResultCode;
  /** Error message */
  message: string;
  /** Error details */
  details?: string;
  /** Field errors (for validation errors) */
  fieldErrors?: Record<string, string>;
  /** Request ID for tracking */
  requestId?: string;
  /** Timestamp */
  timestamp: number;
}

/**
 * Health check response
 */
export interface HealthResponse {
  /** Service status */
  status: "healthy" | "degraded" | "unhealthy";
  /** Service version */
  version: string;
  /** Current timestamp */
  timestamp: number;
  /** Uptime in seconds */
  uptime: number;
  /** Database status */
  database?: "connected" | "disconnected";
  /** WebSocket status */
  websocket?: "connected" | "disconnected";
}

/**
 * Server time response
 */
export interface ServerTime {
  /** Server timestamp (milliseconds) */
  serverTime: number;
  /** Server timezone */
  timezone: string;
}

/**
 * Exchange info response
 */
export interface ExchangeInfo {
  /** Exchange name */
  name: string;
  /** Exchange version */
  version: string;
  /** Supported markets */
  markets: string[];
  /** Supported tokens */
  tokens: string[];
  /** Rate limits */
  rateLimits: RateLimit[];
  /** Server time */
  serverTime: number;
}

/**
 * Rate limit information
 */
export interface RateLimit {
  /** Limit type (e.g., "requests", "orders") */
  type: string;
  /** Interval in milliseconds */
  interval: number;
  /** Limit value */
  limit: number;
  /** Current usage */
  current?: number;
}

/**
 * Account info response
 */
export interface AccountInfoResponse {
  /** Account address */
  address: string;
  /** Account index */
  accountIndex: number;
  /** Current nonce */
  nonce: number;
  /** Account tier */
  tier?: string;
  /** Maker fee rate */
  makerFeeRate: string;
  /** Taker fee rate */
  takerFeeRate: string;
  /** Total trading volume (30 day) */
  tradingVolume30d: string;
  /** Account creation timestamp */
  createdAt: number;
  /** KYC status */
  kycStatus?: "none" | "pending" | "verified" | "rejected";
}

/**
 * Deposit address response
 */
export interface DepositAddress {
  /** Token symbol */
  token: string;
  /** Chain name */
  chain: string;
  /** Deposit address */
  address: string;
  /** Memo/tag (for tokens that require it) */
  memo?: string;
  /** Minimum deposit amount */
  minDeposit: string;
  /** Confirmation blocks required */
  confirmations: number;
}

/**
 * Withdrawal history item
 */
export interface WithdrawalHistoryItem {
  /** Withdrawal ID */
  withdrawalId: string;
  /** Token symbol */
  token: string;
  /** Amount */
  amount: string;
  /** Fee */
  fee: string;
  /** L1 address */
  l1Address: string;
  /** Transaction hash (L1) */
  txHash?: string;
  /** Status */
  status: "pending" | "processing" | "completed" | "failed";
  /** Created at timestamp */
  createdAt: number;
  /** Completed at timestamp */
  completedAt?: number;
}

/**
 * Deposit history item
 */
export interface DepositHistoryItem {
  /** Deposit ID */
  depositId: string;
  /** Token symbol */
  token: string;
  /** Amount */
  amount: string;
  /** L1 transaction hash */
  txHash: string;
  /** Status */
  status: "pending" | "confirming" | "completed" | "failed";
  /** Confirmations */
  confirmations: number;
  /** Required confirmations */
  requiredConfirmations: number;
  /** Created at timestamp */
  createdAt: number;
  /** Completed at timestamp */
  completedAt?: number;
}

/**
 * Transfer history item
 */
export interface TransferHistoryItem {
  /** Transfer ID */
  transferId: string;
  /** Token symbol */
  token: string;
  /** Amount */
  amount: string;
  /** Sender address */
  from: string;
  /** Recipient address */
  to: string;
  /** Memo */
  memo?: string;
  /** Transaction hash */
  txHash: string;
  /** Created at timestamp */
  createdAt: number;
}

/**
 * System status response
 */
export interface SystemStatus {
  /** Overall status */
  status: "operational" | "degraded" | "maintenance" | "outage";
  /** Status message */
  message?: string;
  /** Component statuses */
  components: ComponentStatus[];
  /** Scheduled maintenance */
  scheduledMaintenance?: MaintenanceWindow[];
  /** Last update timestamp */
  lastUpdate: number;
}

/**
 * Component status
 */
export interface ComponentStatus {
  /** Component name */
  name: string;
  /** Status */
  status: "operational" | "degraded" | "down";
  /** Status message */
  message?: string;
  /** Last check timestamp */
  lastCheck: number;
}

/**
 * Maintenance window
 */
export interface MaintenanceWindow {
  /** Maintenance ID */
  id: string;
  /** Title */
  title: string;
  /** Description */
  description: string;
  /** Start time */
  startTime: number;
  /** End time */
  endTime: number;
  /** Affected services */
  affectedServices: string[];
}

/**
 * Fee schedule response
 */
export interface FeeSchedule {
  /** Maker fee tiers */
  makerTiers: FeeTier[];
  /** Taker fee tiers */
  takerTiers: FeeTier[];
  /** Withdrawal fees */
  withdrawalFees: Record<string, string>;
  /** Deposit fees */
  depositFees: Record<string, string>;
}

/**
 * Fee tier
 */
export interface FeeTier {
  /** Tier level */
  tier: number;
  /** Minimum 30-day volume */
  minVolume: string;
  /** Fee rate */
  feeRate: string;
}

/**
 * Funding rate response
 */
export interface FundingRate {
  /** Market symbol */
  symbol: string;
  /** Current funding rate */
  fundingRate: string;
  /** Next funding time */
  nextFundingTime: number;
  /** Funding interval (seconds) */
  fundingInterval: number;
  /** Timestamp */
  timestamp: number;
}

/**
 * Insurance fund response
 */
export interface InsuranceFund {
  /** Total fund size */
  totalSize: string;
  /** Fund breakdown by token */
  balances: Record<string, string>;
  /** Last update timestamp */
  lastUpdate: number;
}
