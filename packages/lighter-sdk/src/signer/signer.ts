/**
 * Transaction Signer
 *
 * This module provides the main signing functionality for Lighter protocol transactions.
 * It combines field element conversion, transaction hashing, and Schnorr signature
 * generation to create valid signatures for L2 transactions.
 *
 * The signing process:
 * 1. Convert transaction data to field elements
 * 2. Concatenate field elements into a message
 * 3. Hash the message using Poseidon2
 * 4. Sign the hash using Schnorr signature over ECgFp5 curve
 *
 * @module signer
 */

import { getCryptoApi, initWasm, isWasmReady, type WasmError } from '@tealstreet/poseidon-crypto';
import { L2CancelOrderTxInfo, L2CreateOrderTxInfo, L2TransferTxInfo, L2WithdrawTxInfo } from '../types';
import { ensureWasmInitialized, normalizePrivateKey } from './key-manager';
import {
  hashAuthToken,
  hashCancelOrderTx,
  hashCreateOrderTx,
  hashTransferTx,
  hashWithdrawTx,
} from './transaction-hasher';

/**
 * Transaction signature result
 */
export interface TransactionSignature {
  /** Hash of the transaction (32 bytes = 64 hex characters) */
  txHash: string;
  /** Schnorr signature (80 bytes = 160 hex characters) */
  signature: string;
}

/**
 * Authentication token result
 */
export interface AuthToken {
  /** The authentication token (signature) */
  token: string;
  /** Message that was signed */
  message: string;
  /** Hash of the message */
  messageHash: string;
  /** Account index */
  accountIndex: number;
  /** API key index */
  apiKeyIndex: number;
  /** Deadline timestamp */
  deadline: bigint;
}

/**
 * Error thrown during signing operations
 */
export class SignerError extends Error {
  constructor(
    message: string,
    public readonly cause?: unknown,
  ) {
    super(message);
    this.name = 'SignerError';
  }
}

/**
 * Check if a value is a WASM error result.
 *
 * @param value - Value to check
 * @returns True if the value is a WasmError object
 */
function isWasmError(value: unknown): value is WasmError {
  return typeof value === 'object' && value !== null && 'error' in value;
}

/**
 * Transaction Signer class for Lighter protocol.
 *
 * This class handles all signing operations for L2 transactions and authentication.
 *
 * @example
 * ```typescript
 * // Create and initialize signer
 * const signer = new TransactionSigner();
 * await signer.initialize();
 *
 * // Sign a create order transaction
 * const createOrderTx = {
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
 *     expiry: 1700000000,
 *     accountIndex: 0,
 *     apiKeyIndex: 0
 *   }
 * };
 *
 * const signature = await signer.signCreateOrder(createOrderTx, privateKey, 1);
 * console.log('TX Hash:', signature.txHash);
 * console.log('Signature:', signature.signature);
 * ```
 */
export class TransactionSigner {
  private initialized = false;

  /**
   * Initialize the signer by loading the WASM crypto module.
   *
   * This must be called once before any signing operations.
   * It's safe to call multiple times - initialization only happens once.
   *
   * @throws {SignerError} If WASM initialization fails
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      await ensureWasmInitialized();
      this.initialized = true;
    } catch (error) {
      throw new SignerError('Failed to initialize signer', error);
    }
  }

  /**
   * Ensure the signer is initialized before signing.
   *
   * @throws {SignerError} If signer is not initialized
   */
  private ensureInitialized(): void {
    if (!this.initialized || !isWasmReady()) {
      throw new SignerError('Signer not initialized. Call initialize() first.');
    }
  }

  /**
   * Sign a raw message hex string.
   *
   * This is a low-level method that directly signs a hex message.
   * For transaction signing, use the specific transaction signing methods instead.
   *
   * @param messageHex - Message as hex string (must be multiple of 8 bytes = 16 hex chars)
   * @param privateKey - Signing private key (80 hex characters)
   * @returns Promise resolving to signature result with hash and signature
   * @throws {SignerError} If signing fails or inputs are invalid
   *
   * @example
   * ```typescript
   * const message = "0100000000000000020000000000000003000000000000000";
   * const sig = await signer.signMessage(message, privateKey);
   * ```
   */
  async signMessage(messageHex: string, privateKey: string): Promise<TransactionSignature> {
    this.ensureInitialized();

    // Normalize private key
    const normalizedKey = normalizePrivateKey(privateKey);

    // Validate message format (must be multiple of 8 bytes)
    const cleanMessage = messageHex.startsWith('0x') ? messageHex.slice(2) : messageHex;
    if (cleanMessage.length % 16 !== 0) {
      throw new SignerError(
        `Message must be a multiple of 8 bytes (16 hex chars). Got ${cleanMessage.length} characters.`,
      );
    }

    const crypto = getCryptoApi();

    // Hash the message using Poseidon2
    const hashResult = crypto.poseidon2Hash(cleanMessage);
    if (isWasmError(hashResult)) {
      throw new SignerError(`Failed to hash message: ${hashResult.error}`);
    }

    // Sign the hash
    const signatureResult = crypto.schnorrSign(hashResult, normalizedKey);
    if (isWasmError(signatureResult)) {
      throw new SignerError(`Failed to sign message: ${signatureResult.error}`);
    }

    return {
      txHash: hashResult,
      signature: signatureResult,
    };
  }

  /**
   * Sign a Create Order transaction.
   *
   * @param tx - Create order transaction information
   * @param privateKey - Signing private key
   * @param chainId - Blockchain chain ID (e.g., 1 for Ethereum mainnet)
   * @returns Promise resolving to signature result
   * @throws {SignerError} If signing fails
   *
   * @example
   * ```typescript
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
   *   opts: { nonce: 1, expiry: 1700000000, accountIndex: 0, apiKeyIndex: 0 }
   * };
   * const sig = await signer.signCreateOrder(tx, privateKey, 1);
   * ```
   */
  async signCreateOrder(tx: L2CreateOrderTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature> {
    this.ensureInitialized();

    const messageHex = hashCreateOrderTx(tx, chainId);
    return this.signMessage(messageHex, privateKey);
  }

  /**
   * Sign a Cancel Order transaction.
   *
   * @param tx - Cancel order transaction information
   * @param privateKey - Signing private key
   * @param chainId - Blockchain chain ID
   * @returns Promise resolving to signature result
   * @throws {SignerError} If signing fails
   *
   * @example
   * ```typescript
   * const tx = {
   *   txType: TxType.L2CancelOrder,
   *   marketId: "0x123abc",
   *   orderId: "0x456def",
   *   opts: { nonce: 2, expiry: 1700000000, accountIndex: 0, apiKeyIndex: 0 }
   * };
   * const sig = await signer.signCancelOrder(tx, privateKey, 1);
   * ```
   */
  async signCancelOrder(tx: L2CancelOrderTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature> {
    this.ensureInitialized();

    const messageHex = hashCancelOrderTx(tx, chainId);
    return this.signMessage(messageHex, privateKey);
  }

  /**
   * Sign a Transfer transaction.
   *
   * @param tx - Transfer transaction information
   * @param privateKey - Signing private key
   * @param chainId - Blockchain chain ID
   * @returns Promise resolving to signature result
   * @throws {SignerError} If signing fails
   *
   * @example
   * ```typescript
   * const tx = {
   *   txType: TxType.L2Transfer,
   *   tokenAddress: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
   *   recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb",
   *   amount: "1000000000",
   *   opts: { nonce: 3, expiry: 1700000000, accountIndex: 0, apiKeyIndex: 0 }
   * };
   * const sig = await signer.signTransfer(tx, privateKey, 1);
   * ```
   */
  async signTransfer(tx: L2TransferTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature> {
    this.ensureInitialized();

    const messageHex = hashTransferTx(tx, chainId);
    return this.signMessage(messageHex, privateKey);
  }

  /**
   * Sign a Withdraw transaction.
   *
   * @param tx - Withdraw transaction information
   * @param privateKey - Signing private key
   * @param chainId - Blockchain chain ID
   * @returns Promise resolving to signature result
   * @throws {SignerError} If signing fails
   *
   * @example
   * ```typescript
   * const tx = {
   *   txType: TxType.L2Withdraw,
   *   tokenAddress: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
   *   amount: "1000000000",
   *   fastWithdraw: true,
   *   opts: { nonce: 4, expiry: 1700000000, accountIndex: 0, apiKeyIndex: 0 }
   * };
   * const sig = await signer.signWithdraw(tx, privateKey, 1);
   * ```
   */
  async signWithdraw(tx: L2WithdrawTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature> {
    this.ensureInitialized();

    const messageHex = hashWithdrawTx(tx, chainId);
    return this.signMessage(messageHex, privateKey);
  }

  /**
   * Create an authentication token for API requests.
   *
   * The authentication token is a signature over a message containing:
   * - Account index
   * - API key index
   * - Deadline timestamp
   * - Chain ID
   *
   * This token can be used in the Authorization header for API requests.
   *
   * @param accountIndex - Sub-account index (0-255)
   * @param apiKeyIndex - API key index (0-255)
   * @param deadline - Unix timestamp when the token expires
   * @param privateKey - Signing private key
   * @param chainId - Blockchain chain ID
   * @returns Promise resolving to authentication token
   * @throws {SignerError} If token creation fails
   *
   * @example
   * ```typescript
   * // Create a token valid for 1 hour
   * const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
   * const auth = await signer.createAuthToken(0, 0, deadline, privateKey, 1);
   *
   * // Use in API request
   * fetch('https://api.lighter.xyz/v1/orders', {
   *   headers: {
   *     'Authorization': `Bearer ${auth.token}`
   *   }
   * });
   * ```
   */
  async createAuthToken(
    accountIndex: number,
    apiKeyIndex: number,
    deadline: bigint,
    privateKey: string,
    chainId: number,
  ): Promise<AuthToken> {
    this.ensureInitialized();

    // Validate parameters
    if (accountIndex < 0 || accountIndex > 255) {
      throw new SignerError(`Account index must be 0-255, got ${accountIndex}`);
    }
    if (apiKeyIndex < 0 || apiKeyIndex > 255) {
      throw new SignerError(`API key index must be 0-255, got ${apiKeyIndex}`);
    }

    // Hash the auth message
    const messageHex = hashAuthToken(accountIndex, apiKeyIndex, deadline, chainId);

    // Sign the message
    const { txHash, signature } = await this.signMessage(messageHex, privateKey);

    return {
      token: signature,
      message: messageHex,
      messageHash: txHash,
      accountIndex,
      apiKeyIndex,
      deadline,
    };
  }

  /**
   * Verify a signature against a public key.
   *
   * This can be used to verify that a signature was created by a specific private key.
   *
   * @param messageHex - Original message that was signed (hex string)
   * @param publicKey - Public key to verify against (80 hex characters)
   * @param signature - Signature to verify (160 hex characters)
   * @returns Promise resolving to true if signature is valid, false otherwise
   * @throws {SignerError} If verification fails (not including invalid signatures)
   *
   * @example
   * ```typescript
   * const isValid = await signer.verifySignature(
   *   messageHex,
   *   publicKey,
   *   signature.signature
   * );
   * if (isValid) {
   *   console.log('Signature is valid');
   * }
   * ```
   */
  async verifySignature(messageHex: string, publicKey: string, signature: string): Promise<boolean> {
    this.ensureInitialized();

    // Validate message format
    const cleanMessage = messageHex.startsWith('0x') ? messageHex.slice(2) : messageHex;
    if (cleanMessage.length % 16 !== 0) {
      throw new SignerError(
        `Message must be a multiple of 8 bytes (16 hex chars). Got ${cleanMessage.length} characters.`,
      );
    }

    const crypto = getCryptoApi();

    // Hash the message
    const hashResult = crypto.poseidon2Hash(cleanMessage);
    if (isWasmError(hashResult)) {
      throw new SignerError(`Failed to hash message: ${hashResult.error}`);
    }

    // Verify the signature
    const verifyResult = crypto.schnorrVerify(hashResult, publicKey, signature);
    if (isWasmError(verifyResult)) {
      throw new SignerError(`Failed to verify signature: ${verifyResult.error}`);
    }

    return verifyResult;
  }
}

/**
 * Create and initialize a new TransactionSigner instance.
 *
 * This is a convenience function that creates a signer and initializes it in one step.
 *
 * @returns Promise resolving to an initialized TransactionSigner
 * @throws {SignerError} If initialization fails
 *
 * @example
 * ```typescript
 * const signer = await createSigner();
 * const signature = await signer.signCreateOrder(tx, privateKey, 1);
 * ```
 */
export async function createSigner(): Promise<TransactionSigner> {
  const signer = new TransactionSigner();
  await signer.initialize();
  return signer;
}