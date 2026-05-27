/**
 * Transaction Hashing Utilities
 *
 * This module provides functions to hash different transaction types for the Lighter protocol.
 * Each transaction is converted to a sequence of field elements (8-byte hex strings) which are
 * then concatenated and hashed using Poseidon2.
 *
 * The field element order matches the Lighter protocol specification and must be maintained
 * exactly for signatures to be valid.
 *
 * @module transaction-hasher
 */

import {
  uint32ToFieldElement,
  uint64ToFieldElement,
  int64ToFieldElement,
  concatenateFieldElements,
} from './field-elements';
import {
  L2CreateOrderTxInfo,
  L2CancelOrderTxInfo,
  L2TransferTxInfo,
  L2WithdrawTxInfo,
} from '../types';

/**
 * Hash a Create Order transaction.
 *
 * The transaction data is converted into a sequence of field elements in the following order:
 * 1. Chain ID (uint32)
 * 2. Transaction type (uint32)
 * 3. Account index (uint32)
 * 4. API key index (uint32)
 * 5. Nonce (uint32)
 * 6. Expiry (uint32)
 * 7. Market ID (as hex string, variable length)
 * 8. Order type (uint32)
 * 9. Order side (uint32)
 * 10. Price (int64)
 * 11. Quantity (int64)
 * 12. Time in force (uint32)
 * 13. Reduce only (uint32, 0 or 1)
 * 14. Post only (uint32, 0 or 1)
 * 15. Stop price (int64, optional)
 *
 * @param tx - Create order transaction information
 * @param chainId - Blockchain chain ID (e.g., 1 for Ethereum mainnet)
 * @returns Concatenated hex string of field elements ready for Poseidon2 hashing
 *
 * @example
 * ```typescript
 * const txInfo = {
 *   txType: TxType.L2CreateOrder,
 *   marketId: "0x123abc",
 *   orderType: OrderType.Limit,
 *   side: OrderSide.Buy,
 *   price: "1000000000000", // 10000.00000000 with 8 decimals
 *   quantity: "100000000", // 1.00000000 with 8 decimals
 *   timeInForce: TimeInForce.GoodTillTime,
 *   reduceOnly: false,
 *   postOnly: false,
 *   opts: {
 *     nonce: 1,
 *     expiry: 1700000000,
 *     accountIndex: 0,
 *     apiKeyIndex: 0
 *   }
 * };
 * const hash = hashCreateOrderTx(txInfo, 1);
 * ```
 */
export function hashCreateOrderTx(tx: L2CreateOrderTxInfo, chainId: number): string {
  const elements: string[] = [];

  // Chain ID
  elements.push(uint32ToFieldElement(chainId));

  // Transaction type
  elements.push(uint32ToFieldElement(tx.txType));

  // Account index
  elements.push(uint32ToFieldElement(tx.opts.accountIndex));

  // API key index
  elements.push(uint32ToFieldElement(tx.opts.apiKeyIndex));

  // Nonce
  elements.push(uint32ToFieldElement(tx.opts.nonce));

  // Expiry
  elements.push(uint32ToFieldElement(tx.opts.expiry));

  // Market ID - convert hex string to field elements
  // Remove 0x prefix if present
  const marketIdHex = tx.marketId.startsWith('0x') ? tx.marketId.slice(2) : tx.marketId;
  // Pad to 8-byte boundary
  const paddedMarketId = marketIdHex.padStart(Math.ceil(marketIdHex.length / 16) * 16, '0');
  // Split into 8-byte chunks
  for (let i = 0; i < paddedMarketId.length; i += 16) {
    elements.push(paddedMarketId.slice(i, i + 16));
  }

  // Order type
  elements.push(uint32ToFieldElement(tx.orderType));

  // Order side
  elements.push(uint32ToFieldElement(tx.side));

  // Price (as int64)
  elements.push(int64ToFieldElement(BigInt(tx.price)));

  // Quantity (as int64)
  elements.push(int64ToFieldElement(BigInt(tx.quantity)));

  // Time in force
  elements.push(uint32ToFieldElement(tx.timeInForce));

  // Reduce only flag (0 or 1)
  elements.push(uint32ToFieldElement(tx.reduceOnly ? 1 : 0));

  // Post only flag (0 or 1)
  elements.push(uint32ToFieldElement(tx.postOnly ? 1 : 0));

  // Stop price (optional)
  if (tx.stopPrice !== undefined) {
    elements.push(int64ToFieldElement(BigInt(tx.stopPrice)));
  }

  return concatenateFieldElements(elements);
}

/**
 * Hash a Cancel Order transaction.
 *
 * The transaction data is converted into field elements in the following order:
 * 1. Chain ID (uint32)
 * 2. Transaction type (uint32)
 * 3. Account index (uint32)
 * 4. API key index (uint32)
 * 5. Nonce (uint32)
 * 6. Expiry (uint32)
 * 7. Market ID (as hex string, variable length)
 * 8. Order ID (as hex string, variable length)
 *
 * @param tx - Cancel order transaction information
 * @param chainId - Blockchain chain ID
 * @returns Concatenated hex string of field elements ready for Poseidon2 hashing
 *
 * @example
 * ```typescript
 * const txInfo = {
 *   txType: TxType.L2CancelOrder,
 *   marketId: "0x123abc",
 *   orderId: "0x456def",
 *   opts: {
 *     nonce: 2,
 *     expiry: 1700000000,
 *     accountIndex: 0,
 *     apiKeyIndex: 0
 *   }
 * };
 * const hash = hashCancelOrderTx(txInfo, 1);
 * ```
 */
export function hashCancelOrderTx(tx: L2CancelOrderTxInfo, chainId: number): string {
  const elements: string[] = [];

  // Chain ID
  elements.push(uint32ToFieldElement(chainId));

  // Transaction type
  elements.push(uint32ToFieldElement(tx.txType));

  // Account index
  elements.push(uint32ToFieldElement(tx.opts.accountIndex));

  // API key index
  elements.push(uint32ToFieldElement(tx.opts.apiKeyIndex));

  // Nonce
  elements.push(uint32ToFieldElement(tx.opts.nonce));

  // Expiry
  elements.push(uint32ToFieldElement(tx.opts.expiry));

  // Market ID - convert hex string to field elements
  const marketIdHex = tx.marketId.startsWith('0x') ? tx.marketId.slice(2) : tx.marketId;
  const paddedMarketId = marketIdHex.padStart(Math.ceil(marketIdHex.length / 16) * 16, '0');
  for (let i = 0; i < paddedMarketId.length; i += 16) {
    elements.push(paddedMarketId.slice(i, i + 16));
  }

  // Order ID - convert hex string to field elements
  const orderIdHex = tx.orderId.startsWith('0x') ? tx.orderId.slice(2) : tx.orderId;
  const paddedOrderId = orderIdHex.padStart(Math.ceil(orderIdHex.length / 16) * 16, '0');
  for (let i = 0; i < paddedOrderId.length; i += 16) {
    elements.push(paddedOrderId.slice(i, i + 16));
  }

  return concatenateFieldElements(elements);
}

/**
 * Hash a Transfer transaction.
 *
 * The transaction data is converted into field elements in the following order:
 * 1. Chain ID (uint32)
 * 2. Transaction type (uint32)
 * 3. Account index (uint32)
 * 4. API key index (uint32)
 * 5. Nonce (uint32)
 * 6. Expiry (uint32)
 * 7. Token address (as hex string, variable length)
 * 8. Recipient address (as hex string, variable length)
 * 9. Amount (int64)
 *
 * @param tx - Transfer transaction information
 * @param chainId - Blockchain chain ID
 * @returns Concatenated hex string of field elements ready for Poseidon2 hashing
 *
 * @example
 * ```typescript
 * const txInfo = {
 *   txType: TxType.L2Transfer,
 *   tokenAddress: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
 *   recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb",
 *   amount: "1000000000",
 *   opts: {
 *     nonce: 3,
 *     expiry: 1700000000,
 *     accountIndex: 0,
 *     apiKeyIndex: 0
 *   }
 * };
 * const hash = hashTransferTx(txInfo, 1);
 * ```
 */
export function hashTransferTx(tx: L2TransferTxInfo, chainId: number): string {
  const elements: string[] = [];

  // Chain ID
  elements.push(uint32ToFieldElement(chainId));

  // Transaction type
  elements.push(uint32ToFieldElement(tx.txType));

  // Account index
  elements.push(uint32ToFieldElement(tx.opts.accountIndex));

  // API key index
  elements.push(uint32ToFieldElement(tx.opts.apiKeyIndex));

  // Nonce
  elements.push(uint32ToFieldElement(tx.opts.nonce));

  // Expiry
  elements.push(uint32ToFieldElement(tx.opts.expiry));

  // Token address - convert hex string to field elements
  const tokenHex = tx.tokenAddress.startsWith('0x') ? tx.tokenAddress.slice(2) : tx.tokenAddress;
  const paddedToken = tokenHex.padStart(Math.ceil(tokenHex.length / 16) * 16, '0');
  for (let i = 0; i < paddedToken.length; i += 16) {
    elements.push(paddedToken.slice(i, i + 16));
  }

  // Recipient address - convert hex string to field elements
  const recipientHex = tx.recipient.startsWith('0x') ? tx.recipient.slice(2) : tx.recipient;
  const paddedRecipient = recipientHex.padStart(Math.ceil(recipientHex.length / 16) * 16, '0');
  for (let i = 0; i < paddedRecipient.length; i += 16) {
    elements.push(paddedRecipient.slice(i, i + 16));
  }

  // Amount (as int64)
  elements.push(int64ToFieldElement(BigInt(tx.amount)));

  return concatenateFieldElements(elements);
}

/**
 * Hash a Withdraw transaction.
 *
 * The transaction data is converted into field elements in the following order:
 * 1. Chain ID (uint32)
 * 2. Transaction type (uint32)
 * 3. Account index (uint32)
 * 4. API key index (uint32)
 * 5. Nonce (uint32)
 * 6. Expiry (uint32)
 * 7. Token address (as hex string, variable length)
 * 8. Amount (int64)
 * 9. L1 recipient (optional, as hex string)
 * 10. Fast withdraw flag (uint32, 0 or 1)
 *
 * @param tx - Withdraw transaction information
 * @param chainId - Blockchain chain ID
 * @returns Concatenated hex string of field elements ready for Poseidon2 hashing
 *
 * @example
 * ```typescript
 * const txInfo = {
 *   txType: TxType.L2Withdraw,
 *   tokenAddress: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
 *   amount: "1000000000",
 *   fastWithdraw: true,
 *   opts: {
 *     nonce: 4,
 *     expiry: 1700000000,
 *     accountIndex: 0,
 *     apiKeyIndex: 0
 *   }
 * };
 * const hash = hashWithdrawTx(txInfo, 1);
 * ```
 */
export function hashWithdrawTx(tx: L2WithdrawTxInfo, chainId: number): string {
  const elements: string[] = [];

  // Chain ID
  elements.push(uint32ToFieldElement(chainId));

  // Transaction type
  elements.push(uint32ToFieldElement(tx.txType));

  // Account index
  elements.push(uint32ToFieldElement(tx.opts.accountIndex));

  // API key index
  elements.push(uint32ToFieldElement(tx.opts.apiKeyIndex));

  // Nonce
  elements.push(uint32ToFieldElement(tx.opts.nonce));

  // Expiry
  elements.push(uint32ToFieldElement(tx.opts.expiry));

  // Token address - convert hex string to field elements
  const tokenHex = tx.tokenAddress.startsWith('0x') ? tx.tokenAddress.slice(2) : tx.tokenAddress;
  const paddedToken = tokenHex.padStart(Math.ceil(tokenHex.length / 16) * 16, '0');
  for (let i = 0; i < paddedToken.length; i += 16) {
    elements.push(paddedToken.slice(i, i + 16));
  }

  // Amount (as int64)
  elements.push(int64ToFieldElement(BigInt(tx.amount)));

  // L1 recipient (optional)
  if (tx.l1Recipient) {
    const recipientHex = tx.l1Recipient.startsWith('0x')
      ? tx.l1Recipient.slice(2)
      : tx.l1Recipient;
    const paddedRecipient = recipientHex.padStart(Math.ceil(recipientHex.length / 16) * 16, '0');
    for (let i = 0; i < paddedRecipient.length; i += 16) {
      elements.push(paddedRecipient.slice(i, i + 16));
    }
  }

  // Fast withdraw flag (0 or 1)
  elements.push(uint32ToFieldElement(tx.fastWithdraw ? 1 : 0));

  return concatenateFieldElements(elements);
}

/**
 * Hash an authentication token request.
 *
 * Creates a message hash for API authentication. The message includes:
 * 1. Account index (uint32)
 * 2. API key index (uint32)
 * 3. Deadline timestamp (uint64)
 * 4. Chain ID (uint32)
 *
 * @param accountIndex - Sub-account index
 * @param apiKeyIndex - API key index
 * @param deadline - Unix timestamp when the token expires
 * @param chainId - Blockchain chain ID
 * @returns Concatenated hex string of field elements ready for Poseidon2 hashing
 *
 * @example
 * ```typescript
 * const authHash = hashAuthToken(0, 0, BigInt(Math.floor(Date.now() / 1000) + 3600), 1);
 * // Create a token valid for 1 hour
 * ```
 */
export function hashAuthToken(
  accountIndex: number,
  apiKeyIndex: number,
  deadline: bigint,
  chainId: number,
): string {
  const elements: string[] = [];

  // Account index
  elements.push(uint32ToFieldElement(accountIndex));

  // API key index
  elements.push(uint32ToFieldElement(apiKeyIndex));

  // Deadline (uint64)
  elements.push(uint64ToFieldElement(deadline));

  // Chain ID
  elements.push(uint32ToFieldElement(chainId));

  return concatenateFieldElements(elements);
}
