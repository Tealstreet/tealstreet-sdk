/**
 * Account API Client
 *
 * Provides methods for retrieving account-related information from the Lighter API.
 * This includes account details, limits, metadata, API keys, liquidations, and PnL data.
 */

import type { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';

/**
 * Account information response
 */
export interface AccountInfo {
  /** Account index */
  accountIndex: string;
  /** L1 address associated with the account */
  l1Address: string;
  /** Account tier */
  tier?: string;
  /** Account status */
  status: string;
  /** Created at timestamp */
  createdAt: number;
  /** Last activity timestamp */
  lastActivityAt?: number;
}

/**
 * Account limits response
 */
export interface AccountLimits {
  /** Account index */
  accountIndex: string;
  /** Maximum open orders */
  maxOpenOrders: number;
  /** Maximum order size */
  maxOrderSize: string;
  /** Maximum position size */
  maxPositionSize: string;
  /** Daily withdrawal limit */
  dailyWithdrawalLimit: string;
  /** Rate limits */
  rateLimits: {
    /** Requests per second */
    requestsPerSecond: number;
    /** Orders per second */
    ordersPerSecond: number;
    /** Orders per day */
    ordersPerDay: number;
  };
}

/**
 * Account metadata response
 */
export interface AccountMetadata {
  /** Account index */
  accountIndex: string;
  /** Account alias/name */
  alias?: string;
  /** Account labels */
  labels?: string[];
  /** Custom metadata fields */
  metadata?: Record<string, any>;
  /** Updated at timestamp */
  updatedAt: number;
}

/**
 * Sub-account information
 */
export interface SubAccount {
  /** Account index */
  accountIndex: string;
  /** Account alias */
  alias?: string;
  /** Account status */
  status: string;
  /** Created at timestamp */
  createdAt: number;
}

/**
 * Accounts by L1 address response
 */
export interface AccountsByL1Address {
  /** L1 address */
  l1Address: string;
  /** List of accounts */
  accounts: SubAccount[];
  /** Total count */
  total: number;
}

/**
 * API key information
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
  /** Account index */
  accountIndex: string;
  /** List of API keys */
  apiKeys: ApiKeyInfo[];
}

/**
 * Liquidation information
 */
export interface Liquidation {
  /** Liquidation ID */
  liquidationId: string;
  /** Account index */
  accountIndex: string;
  /** Market ID */
  marketId?: number;
  /** Liquidated amount */
  amount: string;
  /** Liquidation price */
  price: string;
  /** Timestamp */
  timestamp: number;
  /** Transaction hash */
  txHash?: string;
}

/**
 * Liquidations response
 */
export interface Liquidations {
  /** List of liquidations */
  liquidations: Liquidation[];
  /** Total count */
  total: number;
  /** Has more pages */
  hasMore: boolean;
}

/**
 * PnL data point
 */
export interface PnLDataPoint {
  /** Timestamp */
  timestamp: number;
  /** Realized PnL */
  realizedPnl: string;
  /** Unrealized PnL */
  unrealizedPnl: string;
  /** Total PnL */
  totalPnl: string;
  /** ROI percentage */
  roiPercent?: number;
}

/**
 * PnL response
 */
export interface PnL {
  /** Account index */
  accountIndex: string;
  /** PnL data points */
  data: PnLDataPoint[];
  /** Summary statistics */
  summary?: {
    /** Total realized PnL */
    totalRealizedPnl: string;
    /** Total unrealized PnL */
    totalUnrealizedPnl: string;
    /** Total PnL */
    totalPnl: string;
    /** Win rate percentage */
    winRate?: number;
  };
}

/**
 * Options for getLiquidations
 */
export interface GetLiquidationsOptions {
  /** Limit number of results */
  limit?: number;
  /** Offset for pagination */
  offset?: number;
  /** Filter by market ID */
  marketId?: number;
  /** Start time (unix timestamp) */
  startTime?: number;
  /** End time (unix timestamp) */
  endTime?: number;
}

/**
 * Options for getPnl
 */
export interface GetPnlOptions {
  /** Time period: '1h', '24h', '7d', '30d', '90d', 'all' */
  period?: string;
  /** Market ID (optional, for specific market PnL) */
  marketId?: number;
  /** Start time (unix timestamp) */
  startTime?: number;
  /** End time (unix timestamp) */
  endTime?: number;
}

/**
 * Account API client for retrieving account-related information
 *
 * @example
 * ```typescript
 * const accountApi = new AccountApi(client);
 *
 * // Get account information
 * const account = await accountApi.getAccount(1n);
 * console.log('Account:', account);
 *
 * // Get account limits
 * const limits = await accountApi.getAccountLimits(1n);
 * console.log('Limits:', limits);
 *
 * // Get liquidations
 * const liquidations = await accountApi.getLiquidations(1n, {
 *   limit: 10,
 *   marketId: 1
 * });
 * ```
 */
export class AccountApi {
  /**
   * Creates a new Account API client
   *
   * @param client - Lighter API client instance
   */
  constructor(private readonly client: LighterApiClient) {}

  /**
   * Get account information by account index
   *
   * @param accountIndex - Account index
   * @returns Account information
   *
   * @example
   * ```typescript
   * const account = await accountApi.getAccount(1n);
   * console.log('Account status:', account.status);
   * ```
   */
  public async getAccount(accountIndex: bigint): Promise<AccountInfo> {
    const response = await this.client.get<ApiResponse<AccountInfo>>(
      `/api/v1/accounts/${accountIndex.toString()}`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get account information',
      );
    }

    return response.data;
  }

  /**
   * Get account limits (rate limits, position limits, etc.)
   *
   * @param accountIndex - Account index
   * @returns Account limits
   *
   * @example
   * ```typescript
   * const limits = await accountApi.getAccountLimits(1n);
   * console.log('Max open orders:', limits.maxOpenOrders);
   * console.log('Rate limits:', limits.rateLimits);
   * ```
   */
  public async getAccountLimits(accountIndex: bigint): Promise<AccountLimits> {
    const response = await this.client.get<ApiResponse<AccountLimits>>(
      `/api/v1/accounts/${accountIndex.toString()}/limits`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get account limits',
      );
    }

    return response.data;
  }

  /**
   * Get account metadata (alias, labels, custom fields)
   *
   * @param accountIndex - Account index
   * @returns Account metadata
   *
   * @example
   * ```typescript
   * const metadata = await accountApi.getAccountMetadata(1n);
   * console.log('Account alias:', metadata.alias);
   * console.log('Labels:', metadata.labels);
   * ```
   */
  public async getAccountMetadata(accountIndex: bigint): Promise<AccountMetadata> {
    const response = await this.client.get<ApiResponse<AccountMetadata>>(
      `/api/v1/accounts/${accountIndex.toString()}/metadata`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get account metadata',
      );
    }

    return response.data;
  }

  /**
   * Get all accounts associated with an L1 address
   *
   * @param l1Address - L1 Ethereum address
   * @returns List of accounts
   *
   * @example
   * ```typescript
   * const accounts = await accountApi.getAccountsByL1Address('0x1234...');
   * console.log('Total accounts:', accounts.total);
   * accounts.accounts.forEach(acc => {
   *   console.log('Account:', acc.accountIndex, acc.alias);
   * });
   * ```
   */
  public async getAccountsByL1Address(l1Address: string): Promise<AccountsByL1Address> {
    const response = await this.client.get<ApiResponse<AccountsByL1Address>>(
      '/api/v1/accounts',
      {
        params: { l1Address },
      },
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get accounts by L1 address',
      );
    }

    return response.data;
  }

  /**
   * Get all API keys for an account
   *
   * @param accountIndex - Account index
   * @returns List of API keys
   *
   * @example
   * ```typescript
   * const apiKeys = await accountApi.getApiKeys(1n);
   * apiKeys.apiKeys.forEach(key => {
   *   console.log('API Key:', key.index, key.label, key.isActive);
   * });
   * ```
   */
  public async getApiKeys(accountIndex: bigint): Promise<AccountApiKeys> {
    const response = await this.client.get<ApiResponse<AccountApiKeys>>(
      `/api/v1/accounts/${accountIndex.toString()}/api-keys`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get API keys',
      );
    }

    return response.data;
  }

  /**
   * Get liquidation history for an account
   *
   * @param accountIndex - Account index
   * @param options - Query options (limit, offset, filters)
   * @returns List of liquidations
   *
   * @example
   * ```typescript
   * // Get recent liquidations
   * const liquidations = await accountApi.getLiquidations(1n, {
   *   limit: 20,
   *   marketId: 1
   * });
   *
   * // Get liquidations in date range
   * const historicalLiquidations = await accountApi.getLiquidations(1n, {
   *   startTime: Date.now() - 7 * 24 * 60 * 60 * 1000, // 7 days ago
   *   endTime: Date.now()
   * });
   * ```
   */
  public async getLiquidations(
    accountIndex: bigint,
    options?: GetLiquidationsOptions,
  ): Promise<Liquidations> {
    const params: Record<string, any> = {};

    if (options?.limit !== undefined) {
      params.limit = options.limit;
    }
    if (options?.offset !== undefined) {
      params.offset = options.offset;
    }
    if (options?.marketId !== undefined) {
      params.marketId = options.marketId;
    }
    if (options?.startTime !== undefined) {
      params.startTime = options.startTime;
    }
    if (options?.endTime !== undefined) {
      params.endTime = options.endTime;
    }

    const response = await this.client.get<ApiResponse<Liquidations>>(
      `/api/v1/accounts/${accountIndex.toString()}/liquidations`,
      {
        params,
      },
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get liquidations',
      );
    }

    return response.data;
  }

  /**
   * Get profit and loss (PnL) data for an account
   *
   * @param accountIndex - Account index
   * @param options - Query options (period, market, date range)
   * @returns PnL data with time series
   *
   * @example
   * ```typescript
   * // Get 24h PnL
   * const dailyPnl = await accountApi.getPnl(1n, { period: '24h' });
   * console.log('Total PnL:', dailyPnl.summary?.totalPnl);
   *
   * // Get PnL for specific market
   * const marketPnl = await accountApi.getPnl(1n, {
   *   period: '7d',
   *   marketId: 1
   * });
   *
   * // Get custom date range
   * const customPnl = await accountApi.getPnl(1n, {
   *   startTime: Date.now() - 30 * 24 * 60 * 60 * 1000, // 30 days ago
   *   endTime: Date.now()
   * });
   * ```
   */
  public async getPnl(
    accountIndex: bigint,
    options?: GetPnlOptions,
  ): Promise<PnL> {
    const params: Record<string, any> = {};

    if (options?.period !== undefined) {
      params.period = options.period;
    }
    if (options?.marketId !== undefined) {
      params.marketId = options.marketId;
    }
    if (options?.startTime !== undefined) {
      params.startTime = options.startTime;
    }
    if (options?.endTime !== undefined) {
      params.endTime = options.endTime;
    }

    const response = await this.client.get<ApiResponse<PnL>>(
      `/api/v1/accounts/${accountIndex.toString()}/pnl`,
      {
        params,
      },
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get PnL data',
      );
    }

    return response.data;
  }
}
