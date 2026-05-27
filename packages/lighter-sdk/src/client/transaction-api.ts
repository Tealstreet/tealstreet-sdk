/**
 * Lighter Transaction API Client
 *
 * Provides methods for querying transaction history, deposits, transfers,
 * and transaction details from the Lighter protocol.
 */

import type { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';

/**
 * Transaction status enum
 */
export enum TransactionStatus {
  Pending = 'pending',
  Confirmed = 'confirmed',
  Failed = 'failed',
}

/**
 * Transaction type enum
 */
export enum TransactionType {
  CreateOrder = 'create_order',
  CancelOrder = 'cancel_order',
  Transfer = 'transfer',
  Withdrawal = 'withdrawal',
  Deposit = 'deposit',
}

/**
 * Single transaction data
 */
export interface Transaction {
  /** Transaction hash */
  txHash: string;
  /** Transaction type */
  type: TransactionType;
  /** Account index */
  accountIndex: string;
  /** API key index used */
  apiKeyIndex: number;
  /** Transaction nonce */
  nonce: string;
  /** Transaction status */
  status: TransactionStatus;
  /** Block number (if confirmed) */
  blockNumber?: number;
  /** Block timestamp */
  blockTimestamp?: number;
  /** Transaction sequence number */
  sequence?: number;
  /** Transaction data (hex string) */
  data: string;
  /** Transaction signature */
  signature: string;
  /** Gas used */
  gasUsed?: string;
  /** Error message (if failed) */
  error?: string;
  /** Created at timestamp */
  createdAt: number;
}

/**
 * Transaction list response
 */
export interface TransactionsResponse {
  /** List of transactions */
  transactions: Transaction[];
  /** Total count */
  total?: number;
  /** Cursor for pagination */
  cursor?: string;
}

/**
 * Block transactions response
 */
export interface BlockTransactionsResponse {
  /** Block number */
  blockNumber: number;
  /** Block timestamp */
  blockTimestamp: number;
  /** List of transactions in the block */
  transactions: Transaction[];
  /** Total transactions in block */
  totalTransactions: number;
}

/**
 * Deposit history item
 */
export interface Deposit {
  /** Deposit ID */
  depositId: string;
  /** Account index */
  accountIndex: string;
  /** L1 address */
  l1Address: string;
  /** Token ID */
  tokenId: number;
  /** Deposit amount */
  amount: string;
  /** L1 transaction hash */
  l1TxHash: string;
  /** Deposit status */
  status: 'pending' | 'confirming' | 'completed' | 'failed';
  /** Number of confirmations */
  confirmations: number;
  /** Required confirmations */
  requiredConfirmations: number;
  /** Created at timestamp */
  createdAt: number;
  /** Completed at timestamp */
  completedAt?: number;
}

/**
 * Deposit history response
 */
export interface DepositHistoryResponse {
  /** List of deposits */
  deposits: Deposit[];
  /** Total count */
  total?: number;
  /** Cursor for pagination */
  cursor?: string;
}

/**
 * Transfer history item
 */
export interface Transfer {
  /** Transfer ID */
  transferId: string;
  /** Transaction hash */
  txHash: string;
  /** Sender account index */
  fromAccountIndex: string;
  /** Recipient account index */
  toAccountIndex: string;
  /** Token ID */
  tokenId: number;
  /** Transfer amount */
  amount: string;
  /** Transfer status */
  status: TransactionStatus;
  /** Created at timestamp */
  createdAt: number;
}

/**
 * Transfer history response
 */
export interface TransferHistoryResponse {
  /** List of transfers */
  transfers: Transfer[];
  /** Total count */
  total?: number;
  /** Cursor for pagination */
  cursor?: string;
}

/**
 * Transaction detail response (single transaction)
 */
export interface TransactionDetailResponse {
  /** Transaction data */
  transaction: Transaction;
}

/**
 * Options for getting account transactions
 */
export interface GetAccountTransactionsOptions {
  /** Maximum number of results (default: 50) */
  limit?: number;
  /** Cursor for pagination */
  cursor?: string;
  /** Filter by transaction types */
  types?: TransactionType[];
  /** Authentication token */
  auth?: string;
}

/**
 * Options for getting deposit history
 */
export interface GetDepositHistoryOptions {
  /** L1 address for authentication */
  l1Address?: string;
  /** Authentication token */
  auth?: string;
  /** Cursor for pagination */
  cursor?: string;
  /** Filter parameters */
  filter?: {
    /** Filter by status */
    status?: 'pending' | 'confirming' | 'completed' | 'failed';
    /** Filter by token ID */
    tokenId?: number;
  };
}

/**
 * Options for getting transfer history
 */
export interface GetTransferHistoryOptions {
  /** Authentication token */
  auth?: string;
  /** Cursor for pagination */
  cursor?: string;
  /** Maximum number of results */
  limit?: number;
}

/**
 * Transaction API client for querying transaction history and details
 *
 * @example
 * ```typescript
 * const apiClient = new LighterApiClient({ baseUrl: 'https://api.lighter.xyz' });
 * const txApi = new TransactionApi(apiClient);
 *
 * // Get account transactions
 * const accountTxs = await txApi.getAccountTransactions(1n, { limit: 10 });
 *
 * // Get a specific transaction
 * const tx = await txApi.getTransaction('0x123...');
 * ```
 */
export class TransactionApi {
  private readonly client: LighterApiClient;

  /**
   * Creates a new Transaction API client
   *
   * @param client - The base Lighter API client instance
   */
  constructor(client: LighterApiClient) {
    this.client = client;
  }

  /**
   * Get all transactions for a specific account
   *
   * @param accountIndex - Account index to query
   * @param opts - Query options
   * @returns List of transactions
   *
   * @example
   * ```typescript
   * const transactions = await txApi.getAccountTransactions(1n, {
   *   limit: 20,
   *   types: [TransactionType.CreateOrder, TransactionType.CancelOrder]
   * });
   * ```
   */
  public async getAccountTransactions(
    accountIndex: bigint,
    opts?: GetAccountTransactionsOptions,
  ): Promise<TransactionsResponse> {
    const params: Record<string, any> = {
      limit: opts?.limit,
      cursor: opts?.cursor,
      auth: opts?.auth,
    };

    // Add transaction types filter if provided
    if (opts?.types && opts.types.length > 0) {
      params.types = opts.types.join(',');
    }

    const response = await this.client.get<ApiResponse<TransactionsResponse>>(
      `/api/v1/accounts/${accountIndex.toString()}/transactions`,
      {
        params,
      },
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get account transactions',
      );
    }

    return response.data;
  }

  /**
   * Get all transactions in a specific block
   *
   * @param blockNumber - Block number to query
   * @returns List of transactions in the block
   *
   * @example
   * ```typescript
   * const blockTxs = await txApi.getBlockTransactions(12345);
   * console.log(`Block ${blockTxs.blockNumber} has ${blockTxs.totalTransactions} transactions`);
   * ```
   */
  public async getBlockTransactions(
    blockNumber: number,
  ): Promise<BlockTransactionsResponse> {
    const response = await this.client.get<ApiResponse<BlockTransactionsResponse>>(
      `/api/v1/blocks/${blockNumber}/transactions`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get block transactions',
      );
    }

    return response.data;
  }

  /**
   * Get deposit history for an account
   *
   * @param accountIndex - Account index to query
   * @param opts - Query options including authentication
   * @returns List of deposits
   *
   * @example
   * ```typescript
   * const deposits = await txApi.getDepositHistory(1n, {
   *   l1Address: '0xabc...',
   *   auth: 'auth-token',
   *   filter: { status: 'completed' }
   * });
   * ```
   */
  public async getDepositHistory(
    accountIndex: bigint,
    opts?: GetDepositHistoryOptions,
  ): Promise<DepositHistoryResponse> {
    const params: Record<string, any> = {
      cursor: opts?.cursor,
      auth: opts?.auth,
      l1Address: opts?.l1Address,
    };

    // Add filter parameters if provided
    if (opts?.filter) {
      if (opts.filter.status) {
        params.status = opts.filter.status;
      }
      if (opts.filter.tokenId !== undefined) {
        params.tokenId = opts.filter.tokenId;
      }
    }

    const response = await this.client.get<ApiResponse<DepositHistoryResponse>>(
      `/api/v1/accounts/${accountIndex.toString()}/deposits`,
      {
        params,
      },
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get deposit history',
      );
    }

    return response.data;
  }

  /**
   * Get transfer history for an account
   *
   * @param accountIndex - Account index to query
   * @param opts - Query options including authentication
   * @returns List of transfers
   *
   * @example
   * ```typescript
   * const transfers = await txApi.getTransferHistory(1n, {
   *   auth: 'auth-token',
   *   limit: 50
   * });
   * ```
   */
  public async getTransferHistory(
    accountIndex: bigint,
    opts?: GetTransferHistoryOptions,
  ): Promise<TransferHistoryResponse> {
    const params: Record<string, any> = {
      cursor: opts?.cursor,
      auth: opts?.auth,
      limit: opts?.limit,
    };

    const response = await this.client.get<ApiResponse<TransferHistoryResponse>>(
      `/api/v1/accounts/${accountIndex.toString()}/transfers`,
      {
        params,
      },
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get transfer history',
      );
    }

    return response.data;
  }

  /**
   * Get a specific transaction by its hash or sequence number
   *
   * @param txHashOrSeq - Transaction hash (hex string) or sequence number
   * @returns Transaction details
   *
   * @example
   * ```typescript
   * // Get by transaction hash
   * const tx1 = await txApi.getTransaction('0x123abc...');
   *
   * // Get by sequence number
   * const tx2 = await txApi.getTransaction(12345);
   * ```
   */
  public async getTransaction(
    txHashOrSeq: string | number,
  ): Promise<Transaction> {
    const identifier = typeof txHashOrSeq === 'number'
      ? txHashOrSeq.toString()
      : txHashOrSeq;

    const response = await this.client.get<ApiResponse<TransactionDetailResponse>>(
      `/api/v1/transactions/${identifier}`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get transaction',
      );
    }

    return response.data.transaction;
  }

  /**
   * Get a transaction by its L1 transaction hash
   *
   * This is useful for tracking L1 deposits and finding their corresponding L2 transactions.
   *
   * @param l1TxHash - L1 transaction hash (hex string)
   * @returns Transaction details
   *
   * @example
   * ```typescript
   * const l2Tx = await txApi.getTransactionFromL1Hash('0xdef456...');
   * console.log('L1 deposit resulted in L2 transaction:', l2Tx.txHash);
   * ```
   */
  public async getTransactionFromL1Hash(
    l1TxHash: string,
  ): Promise<Transaction> {
    const response = await this.client.get<ApiResponse<TransactionDetailResponse>>(
      `/api/v1/transactions/l1/${l1TxHash}`,
    );

    if (!response.success || !response.data) {
      throw new Error(
        response.error?.message || 'Failed to get transaction from L1 hash',
      );
    }

    return response.data.transaction;
  }
}
