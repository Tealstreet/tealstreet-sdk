/**
 * Validation utilities for Lighter SDK
 *
 * Provides validation functions for transaction parameters and other inputs.
 */

import type { CreateOrderParams } from '../client/transaction-client';
import {
  MaxAccountIndex,
  MaxApiKeyIndex,
  MaxNonce,
  MaxOrderSize,
  MinOrderSize,
} from '../types';

/**
 * Validation error class
 */
export class ValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ValidationError';
  }
}

/**
 * Validate account index
 *
 * @param index - Account index to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateAccountIndex(0n); // OK
 * validateAccountIndex(255n); // OK
 * validateAccountIndex(256n); // throws ValidationError
 * validateAccountIndex(-1n); // throws ValidationError
 * ```
 */
export function validateAccountIndex(index: bigint): void {
  if (typeof index !== 'bigint') {
    throw new ValidationError(
      `Account index must be a BigInt, got ${typeof index}`
    );
  }

  if (index < 0n) {
    throw new ValidationError(
      `Account index must be non-negative, got ${index}`
    );
  }

  if (index > BigInt(MaxAccountIndex)) {
    throw new ValidationError(
      `Account index must be at most ${MaxAccountIndex}, got ${index}`
    );
  }
}

/**
 * Validate API key index
 *
 * @param index - API key index to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateApiKeyIndex(0); // OK
 * validateApiKeyIndex(255); // OK
 * validateApiKeyIndex(256); // throws ValidationError
 * validateApiKeyIndex(-1); // throws ValidationError
 * validateApiKeyIndex(1.5); // throws ValidationError
 * ```
 */
export function validateApiKeyIndex(index: number): void {
  if (typeof index !== 'number') {
    throw new ValidationError(
      `API key index must be a number, got ${typeof index}`
    );
  }

  if (!Number.isInteger(index)) {
    throw new ValidationError(
      `API key index must be an integer, got ${index}`
    );
  }

  if (index < 0) {
    throw new ValidationError(
      `API key index must be non-negative, got ${index}`
    );
  }

  if (index > MaxApiKeyIndex) {
    throw new ValidationError(
      `API key index must be at most ${MaxApiKeyIndex}, got ${index}`
    );
  }
}

/**
 * Validate nonce
 *
 * @param nonce - Nonce to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateNonce(0n); // OK
 * validateNonce(4294967295n); // OK (uint32 max)
 * validateNonce(4294967296n); // throws ValidationError
 * validateNonce(-1n); // throws ValidationError
 * ```
 */
export function validateNonce(nonce: bigint): void {
  if (typeof nonce !== 'bigint') {
    throw new ValidationError(`Nonce must be a BigInt, got ${typeof nonce}`);
  }

  if (nonce < 0n) {
    throw new ValidationError(`Nonce must be non-negative, got ${nonce}`);
  }

  if (nonce > BigInt(MaxNonce)) {
    throw new ValidationError(
      `Nonce must be at most ${MaxNonce} (uint32 max), got ${nonce}`
    );
  }
}

/**
 * Validate order ID
 *
 * @param orderId - Order ID to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateOrderId(1n); // OK
 * validateOrderId(0n); // throws ValidationError (must be positive)
 * validateOrderId(-1n); // throws ValidationError
 * ```
 */
export function validateOrderId(orderId: bigint): void {
  if (typeof orderId !== 'bigint') {
    throw new ValidationError(
      `Order ID must be a BigInt, got ${typeof orderId}`
    );
  }

  if (orderId <= 0n) {
    throw new ValidationError(
      `Order ID must be positive, got ${orderId}`
    );
  }
}

/**
 * Validate market ID
 *
 * @param marketId - Market ID to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateMarketId(1); // OK
 * validateMarketId(0); // throws ValidationError (must be positive)
 * validateMarketId(-1); // throws ValidationError
 * validateMarketId(1.5); // throws ValidationError
 * ```
 */
export function validateMarketId(marketId: number): void {
  if (typeof marketId !== 'number') {
    throw new ValidationError(
      `Market ID must be a number, got ${typeof marketId}`
    );
  }

  if (!Number.isInteger(marketId)) {
    throw new ValidationError(
      `Market ID must be an integer, got ${marketId}`
    );
  }

  if (marketId <= 0) {
    throw new ValidationError(
      `Market ID must be positive, got ${marketId}`
    );
  }
}

/**
 * Validate order price
 *
 * @param price - Price to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validatePrice(1000000n); // OK
 * validatePrice(0n); // throws ValidationError (must be positive)
 * validatePrice(-1n); // throws ValidationError
 * ```
 */
export function validatePrice(price: bigint): void {
  if (typeof price !== 'bigint') {
    throw new ValidationError(`Price must be a BigInt, got ${typeof price}`);
  }

  if (price <= 0n) {
    throw new ValidationError(`Price must be positive, got ${price}`);
  }
}

/**
 * Validate order amount
 *
 * @param amount - Amount to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateAmount(1000000n); // OK
 * validateAmount(0n); // throws ValidationError (must be >= MinOrderSize)
 * validateAmount(-1n); // throws ValidationError
 * ```
 */
export function validateAmount(amount: bigint): void {
  if (typeof amount !== 'bigint') {
    throw new ValidationError(`Amount must be a BigInt, got ${typeof amount}`);
  }

  if (amount < MinOrderSize) {
    throw new ValidationError(
      `Amount must be at least ${MinOrderSize}, got ${amount}`
    );
  }

  if (amount > MaxOrderSize) {
    throw new ValidationError(
      `Amount must be at most ${MaxOrderSize}, got ${amount}`
    );
  }
}

/**
 * Validate order parameters
 *
 * @param params - Order parameters to validate
 * @throws ValidationError if any parameter is invalid
 *
 * @example
 * ```typescript
 * validateOrderParams({
 *   orderId: 123n,
 *   marketId: 1,
 *   isBuy: true,
 *   price: 1000000n,
 *   amount: 500000n
 * }); // OK
 * ```
 */
export function validateOrderParams(params: CreateOrderParams): void {
  // Validate required fields
  if (!params.orderId && params.orderId !== 0n) {
    throw new ValidationError('Order ID is required');
  }

  if (!params.marketId && params.marketId !== 0) {
    throw new ValidationError('Market ID is required');
  }

  if (typeof params.isBuy !== 'boolean') {
    throw new ValidationError('isBuy must be a boolean');
  }

  if (!params.price && params.price !== 0n) {
    throw new ValidationError('Price is required');
  }

  if (!params.amount && params.amount !== 0n) {
    throw new ValidationError('Amount is required');
  }

  // Validate individual fields
  validateOrderId(params.orderId);
  validateMarketId(params.marketId);
  validatePrice(params.price);
  validateAmount(params.amount);

  // Validate optional boolean flags
  if (params.postOnly !== undefined && typeof params.postOnly !== 'boolean') {
    throw new ValidationError('postOnly must be a boolean');
  }

  if (params.reduceOnly !== undefined && typeof params.reduceOnly !== 'boolean') {
    throw new ValidationError('reduceOnly must be a boolean');
  }

  // Validate incompatible flags
  if (params.postOnly && params.reduceOnly) {
    throw new ValidationError('Cannot set both postOnly and reduceOnly');
  }
}

/**
 * Validate private key format
 *
 * @param privateKey - Private key to validate (hex string)
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validatePrivateKey('0x1234...'); // OK (64 hex chars after 0x)
 * validatePrivateKey('1234...'); // throws ValidationError (missing 0x)
 * validatePrivateKey('0x123'); // throws ValidationError (too short)
 * ```
 */
export function validatePrivateKey(privateKey: string): void {
  if (typeof privateKey !== 'string') {
    throw new ValidationError(
      `Private key must be a string, got ${typeof privateKey}`
    );
  }

  if (!privateKey.startsWith('0x')) {
    throw new ValidationError(
      'Private key must start with 0x prefix'
    );
  }

  const hex = privateKey.slice(2);

  if (hex.length !== 64) {
    throw new ValidationError(
      `Private key must be 64 hex characters (32 bytes), got ${hex.length}`
    );
  }

  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw new ValidationError(
      'Private key must contain only hexadecimal characters'
    );
  }
}

/**
 * Validate public key format
 *
 * @param publicKey - Public key to validate (hex string)
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validatePublicKey('0x1234...'); // OK (64 hex chars after 0x)
 * validatePublicKey('1234...'); // throws ValidationError (missing 0x)
 * ```
 */
export function validatePublicKey(publicKey: string): void {
  if (typeof publicKey !== 'string') {
    throw new ValidationError(
      `Public key must be a string, got ${typeof publicKey}`
    );
  }

  if (!publicKey.startsWith('0x')) {
    throw new ValidationError(
      'Public key must start with 0x prefix'
    );
  }

  const hex = publicKey.slice(2);

  if (hex.length !== 64) {
    throw new ValidationError(
      `Public key must be 64 hex characters (32 bytes), got ${hex.length}`
    );
  }

  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw new ValidationError(
      'Public key must contain only hexadecimal characters'
    );
  }
}

/**
 * Validate address format
 *
 * @param address - Address to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateAddress('0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb'); // OK
 * validateAddress('0xinvalid'); // throws ValidationError
 * ```
 */
export function validateAddress(address: string): void {
  if (typeof address !== 'string') {
    throw new ValidationError(
      `Address must be a string, got ${typeof address}`
    );
  }

  if (!address.startsWith('0x')) {
    throw new ValidationError(
      'Address must start with 0x prefix'
    );
  }

  if (address.length !== 42) {
    throw new ValidationError(
      `Address must be 42 characters (0x + 40 hex chars), got ${address.length}`
    );
  }

  const hex = address.slice(2);
  if (!/^[0-9a-fA-F]{40}$/.test(hex)) {
    throw new ValidationError(
      'Address must contain only hexadecimal characters'
    );
  }
}

/**
 * Validate deadline timestamp
 *
 * @param deadline - Deadline timestamp (unix timestamp in seconds)
 * @param minFuture - Minimum seconds in the future (default: 60)
 * @throws ValidationError if invalid or in the past
 *
 * @example
 * ```typescript
 * const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
 * validateDeadline(deadline); // OK (1 hour from now)
 * validateDeadline(0n); // throws ValidationError (in the past)
 * ```
 */
export function validateDeadline(deadline: bigint, minFuture: number = 60): void {
  if (typeof deadline !== 'bigint') {
    throw new ValidationError(
      `Deadline must be a BigInt, got ${typeof deadline}`
    );
  }

  if (deadline <= 0n) {
    throw new ValidationError(
      `Deadline must be positive, got ${deadline}`
    );
  }

  const now = BigInt(Math.floor(Date.now() / 1000));
  const minDeadline = now + BigInt(minFuture);

  if (deadline < minDeadline) {
    throw new ValidationError(
      `Deadline must be at least ${minFuture} seconds in the future. ` +
      `Current: ${deadline}, minimum required: ${minDeadline}`
    );
  }
}

/**
 * Validate chain ID
 *
 * @param chainId - Chain ID to validate
 * @throws ValidationError if invalid
 *
 * @example
 * ```typescript
 * validateChainId(1); // OK (mainnet)
 * validateChainId(0); // throws ValidationError
 * validateChainId(-1); // throws ValidationError
 * ```
 */
export function validateChainId(chainId: number): void {
  if (typeof chainId !== 'number') {
    throw new ValidationError(
      `Chain ID must be a number, got ${typeof chainId}`
    );
  }

  if (!Number.isInteger(chainId)) {
    throw new ValidationError(
      `Chain ID must be an integer, got ${chainId}`
    );
  }

  if (chainId <= 0) {
    throw new ValidationError(
      `Chain ID must be positive, got ${chainId}`
    );
  }
}
