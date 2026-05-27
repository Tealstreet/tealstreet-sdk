/**
 * Lighter SDK Signer Module
 *
 * This module provides complete signing functionality for Lighter protocol transactions.
 *
 * ## Features
 * - Field element conversion utilities for building transaction messages
 * - Transaction hashing functions for all L2 transaction types
 * - Key generation and management using Schnorr signatures
 * - Transaction signing with Poseidon2 hash and ECgFp5 curve
 * - Authentication token creation for API requests
 *
 * ## Quick Start
 *
 * ```typescript
 * import { createSigner, generateKeyPair } from '@tealstreet/lighter-sdk/signer';
 * import { TxType, OrderType, OrderSide, TimeInForce } from '@tealstreet/lighter-sdk/types';
 *
 * // Generate a new key pair
 * const keys = await generateKeyPair();
 * console.log('Private key:', keys.privateKey);
 * console.log('Public key:', keys.publicKey);
 *
 * // Create and initialize signer
 * const signer = await createSigner();
 *
 * // Create a transaction
 * const tx = {
 *   txType: TxType.L2CreateOrder,
 *   marketId: "0x123abc",
 *   orderType: OrderType.Limit,
 *   side: OrderSide.Buy,
 *   price: "1000000000000",
 *   quantity: "100000000",
 *   timeInForce: TimeInForce.GoodTillTime,
 *   reduceOnly: false,
 *   postOnly: false,
 *   opts: {
 *     nonce: 1,
 *     expiry: Math.floor(Date.now() / 1000) + 3600,
 *     accountIndex: 0,
 *     apiKeyIndex: 0
 *   }
 * };
 *
 * // Sign the transaction
 * const signature = await signer.signCreateOrder(tx, keys.privateKey, 1);
 * console.log('TX Hash:', signature.txHash);
 * console.log('Signature:', signature.signature);
 *
 * // Create an auth token for API requests
 * const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
 * const authToken = await signer.createAuthToken(0, 0, deadline, keys.privateKey, 1);
 *
 * // Use in API request
 * fetch('https://api.lighter.xyz/v1/orders', {
 *   headers: {
 *     'Authorization': `Bearer ${authToken.token}`
 *   }
 * });
 * ```
 *
 * @module signer
 */

// Field element conversion utilities
export {
  uint32ToFieldElement,
  uint64ToFieldElement,
  int64ToFieldElement,
  splitUint64,
  hexToFieldElement,
  concatenateFieldElements,
} from './field-elements';

// Transaction hashing functions
export {
  hashCreateOrderTx,
  hashCancelOrderTx,
  hashTransferTx,
  hashWithdrawTx,
  hashAuthToken,
} from './transaction-hasher';

// Key management utilities
export {
  generateKeyPair,
  derivePublicKey,
  isValidPrivateKey,
  isValidPublicKey,
  normalizePrivateKey,
  normalizePublicKey,
  secureCompareKeys,
  ensureWasmInitialized,
  KeyManagerError,
} from './key-manager';
export type { KeyPair } from './key-manager';

// Main signer class
export {
  TransactionSigner,
  createSigner,
  SignerError,
} from './signer';
export type {
  TransactionSignature,
  AuthToken,
} from './signer';
