/**
 * Lighter SDK Utility Functions
 *
 * Common utility functions for working with the Lighter SDK.
 */

import { MaxAccountIndex, MaxApiKeyIndex, MaxNonce } from '../types';

/**
 * Options for retry function
 */
export interface RetryOptions {
  /** Maximum number of retry attempts (default: 3) */
  maxAttempts?: number;
  /** Initial delay in milliseconds (default: 1000) */
  initialDelay?: number;
  /** Maximum delay in milliseconds (default: 30000) */
  maxDelay?: number;
  /** Backoff multiplier (default: 2) */
  backoffMultiplier?: number;
  /** Function to determine if error is retryable (default: always retry) */
  shouldRetry?: (error: unknown) => boolean;
}

/**
 * Convert BigInt to hex string with optional padding
 *
 * @param value - BigInt value to convert
 * @param bytes - Optional number of bytes for padding (default: no padding)
 * @returns Hex string with '0x' prefix
 *
 * @example
 * ```typescript
 * bigIntToHex(255n) // '0xff'
 * bigIntToHex(255n, 4) // '0x000000ff'
 * bigIntToHex(BigInt('1000000000000000000')) // '0xde0b6b3a7640000'
 * ```
 */
export function bigIntToHex(value: bigint, bytes?: number): string {
  let hex = value.toString(16);

  if (bytes !== undefined) {
    const targetLength = bytes * 2;
    hex = hex.padStart(targetLength, '0');
  }

  return `0x${hex}`;
}

/**
 * Convert hex string to BigInt
 *
 * @param hex - Hex string (with or without '0x' prefix)
 * @returns BigInt value
 * @throws Error if hex string is invalid
 *
 * @example
 * ```typescript
 * hexToBigInt('0xff') // 255n
 * hexToBigInt('ff') // 255n
 * hexToBigInt('0xde0b6b3a7640000') // 1000000000000000000n
 * ```
 */
export function hexToBigInt(hex: string): bigint {
  // Remove 0x prefix if present
  const cleanHex = hex.startsWith('0x') ? hex.slice(2) : hex;

  // Validate hex string
  if (!/^[0-9a-fA-F]*$/.test(cleanHex)) {
    throw new Error(`Invalid hex string: ${hex}`);
  }

  if (cleanHex.length === 0) {
    return 0n;
  }

  return BigInt(`0x${cleanHex}`);
}

/**
 * Validate Ethereum-style address
 *
 * @param address - Address to validate
 * @returns True if address is valid
 *
 * @example
 * ```typescript
 * validateAddress('0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb')
 * validateAddress('0xinvalid') // false
 * ```
 */
export function validateAddress(address: string): boolean {
  // Must start with 0x
  if (!address.startsWith('0x')) {
    return false;
  }

  // Must be 42 characters (0x + 40 hex chars)
  if (address.length !== 42) {
    return false;
  }

  // Must contain only hex characters after 0x
  const hex = address.slice(2);
  return /^[0-9a-fA-F]{40}$/.test(hex);
}

/**
 * Format amount for display with proper decimal places
 *
 * @param amount - Amount in base units (BigInt)
 * @param decimals - Number of decimal places
 * @returns Formatted string
 *
 * @example
 * ```typescript
 * formatAmount(1000000000000000000n, 18) // '1.0'
 * formatAmount(123456789n, 8) // '1.23456789'
 * formatAmount(100000000n, 8) // '1.0'
 * ```
 */
export function formatAmount(amount: bigint, decimals: number): string {
  if (decimals < 0) {
    throw new Error('Decimals must be non-negative');
  }

  if (decimals === 0) {
    return amount.toString();
  }

  const divisor = BigInt(10) ** BigInt(decimals);
  const wholePart = amount / divisor;
  const fractionalPart = amount % divisor;

  if (fractionalPart === 0n) {
    return wholePart.toString() + '.0';
  }

  // Pad fractional part to correct length
  const fractionalStr = fractionalPart.toString().padStart(decimals, '0');

  // Remove trailing zeros
  const trimmedFractional = fractionalStr.replace(/0+$/, '');

  return `${wholePart}.${trimmedFractional}`;
}

/**
 * Parse amount string to base units (BigInt)
 *
 * @param amount - Amount string (e.g., "1.5", "1000")
 * @param decimals - Number of decimal places
 * @returns Amount in base units
 * @throws Error if amount string is invalid
 *
 * @example
 * ```typescript
 * parseAmount('1.0', 18) // 1000000000000000000n
 * parseAmount('1.23456789', 8) // 123456789n
 * parseAmount('100', 8) // 10000000000n
 * ```
 */
export function parseAmount(amount: string, decimals: number): bigint {
  if (decimals < 0) {
    throw new Error('Decimals must be non-negative');
  }

  // Validate amount format
  if (!/^[0-9]+\.?[0-9]*$/.test(amount)) {
    throw new Error(`Invalid amount format: ${amount}`);
  }

  const parts = amount.split('.');
  const wholePart = parts[0] || '0';
  const fractionalPart = parts[1] || '';

  // Check if fractional part has too many decimals
  if (fractionalPart.length > decimals) {
    throw new Error(
      `Amount has too many decimal places. Expected ${decimals}, got ${fractionalPart.length}`
    );
  }

  // Pad fractional part to correct length
  const paddedFractional = fractionalPart.padEnd(decimals, '0');

  // Combine and convert to BigInt
  const combinedStr = wholePart + paddedFractional;
  return BigInt(combinedStr);
}

/**
 * Async sleep utility
 *
 * @param ms - Milliseconds to sleep
 * @returns Promise that resolves after the specified time
 *
 * @example
 * ```typescript
 * await sleep(1000); // Sleep for 1 second
 * ```
 */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Retry a function with exponential backoff
 *
 * @param fn - Async function to retry
 * @param options - Retry options
 * @returns Result of the function
 * @throws Last error if all retries fail
 *
 * @example
 * ```typescript
 * const result = await retry(
 *   async () => await api.fetchData(),
 *   {
 *     maxAttempts: 5,
 *     initialDelay: 1000,
 *     shouldRetry: (error) => error.code === 'NETWORK_ERROR'
 *   }
 * );
 * ```
 */
export async function retry<T>(
  fn: () => Promise<T>,
  options: RetryOptions = {}
): Promise<T> {
  const {
    maxAttempts = 3,
    initialDelay = 1000,
    maxDelay = 30000,
    backoffMultiplier = 2,
    shouldRetry = () => true,
  } = options;

  let lastError: unknown;
  let delay = initialDelay;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error;

      // Don't retry if this is the last attempt or error is not retryable
      if (attempt === maxAttempts || !shouldRetry(error)) {
        throw error;
      }

      // Wait before retrying
      await sleep(delay);

      // Increase delay with backoff multiplier, capped at maxDelay
      delay = Math.min(delay * backoffMultiplier, maxDelay);
    }
  }

  // This should never be reached, but TypeScript needs it
  throw lastError;
}

/**
 * Clamp a BigInt value between min and max
 *
 * @param value - Value to clamp
 * @param min - Minimum value
 * @param max - Maximum value
 * @returns Clamped value
 *
 * @example
 * ```typescript
 * clampBigInt(5n, 0n, 10n) // 5n
 * clampBigInt(-5n, 0n, 10n) // 0n
 * clampBigInt(15n, 0n, 10n) // 10n
 * ```
 */
export function clampBigInt(value: bigint, min: bigint, max: bigint): bigint {
  if (min > max) {
    throw new Error('Min must be less than or equal to max');
  }

  if (value < min) {
    return min;
  }

  if (value > max) {
    return max;
  }

  return value;
}

/**
 * Check if a BigInt value is within a range (inclusive)
 *
 * @param value - Value to check
 * @param min - Minimum value (inclusive)
 * @param max - Maximum value (inclusive)
 * @returns True if value is in range
 *
 * @example
 * ```typescript
 * isInRange(5n, 0n, 10n) // true
 * isInRange(-1n, 0n, 10n) // false
 * isInRange(11n, 0n, 10n) // false
 * ```
 */
export function isInRange(value: bigint, min: bigint, max: bigint): boolean {
  return value >= min && value <= max;
}

/**
 * Safely convert number to BigInt, checking for decimals
 *
 * @param value - Number to convert
 * @returns BigInt value
 * @throws Error if number has decimal places
 *
 * @example
 * ```typescript
 * numberToBigInt(123) // 123n
 * numberToBigInt(123.0) // 123n
 * numberToBigInt(123.5) // throws Error
 * ```
 */
export function numberToBigInt(value: number): bigint {
  if (!Number.isInteger(value)) {
    throw new Error(`Cannot convert decimal number ${value} to BigInt`);
  }

  if (!Number.isSafeInteger(value)) {
    throw new Error(`Number ${value} exceeds safe integer range`);
  }

  return BigInt(value);
}

/**
 * Calculate percentage of a BigInt value
 *
 * @param value - Base value
 * @param percentage - Percentage (e.g., 5 for 5%)
 * @param decimals - Decimal places for precision (default: 0)
 * @returns Percentage of value
 *
 * @example
 * ```typescript
 * percentageOf(1000n, 5) // 50n (5% of 1000)
 * percentageOf(1000n, 2.5, 1) // 25n (2.5% of 1000)
 * ```
 */
export function percentageOf(
  value: bigint,
  percentage: number,
  decimals: number = 0
): bigint {
  if (percentage < 0) {
    throw new Error('Percentage must be non-negative');
  }

  if (decimals < 0) {
    throw new Error('Decimals must be non-negative');
  }

  // Convert percentage to BigInt with precision
  const multiplier = BigInt(10 ** decimals);
  const percentageBigInt = BigInt(Math.floor(percentage * 10 ** decimals));

  // Calculate percentage
  const result = (value * percentageBigInt) / (100n * multiplier);

  return result;
}

/**
 * Truncate a string to a maximum length with ellipsis
 *
 * @param str - String to truncate
 * @param maxLength - Maximum length
 * @param ellipsis - Ellipsis string (default: '...')
 * @returns Truncated string
 *
 * @example
 * ```typescript
 * truncate('0x1234567890abcdef', 10) // '0x1234567...'
 * truncate('short', 10) // 'short'
 * ```
 */
export function truncate(str: string, maxLength: number, ellipsis: string = '...'): string {
  if (str.length <= maxLength) {
    return str;
  }

  const truncateLength = maxLength - ellipsis.length;
  if (truncateLength < 0) {
    return ellipsis.slice(0, maxLength);
  }

  return str.slice(0, truncateLength) + ellipsis;
}

/**
 * Format address for display (show first and last characters)
 *
 * @param address - Address to format
 * @param startChars - Number of characters to show at start (default: 6)
 * @param endChars - Number of characters to show at end (default: 4)
 * @returns Formatted address
 *
 * @example
 * ```typescript
 * formatAddress('0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb')
 * // '0x742d...0bEb'
 * ```
 */
export function formatAddress(
  address: string,
  startChars: number = 6,
  endChars: number = 4
): string {
  if (address.length <= startChars + endChars) {
    return address;
  }

  return `${address.slice(0, startChars)}...${address.slice(-endChars)}`;
}

/**
 * Check if value is a valid account index
 *
 * @param value - Value to check
 * @returns True if valid account index
 */
export function isValidAccountIndex(value: bigint): boolean {
  return value >= 0n && value <= BigInt(MaxAccountIndex);
}

/**
 * Check if value is a valid API key index
 *
 * @param value - Value to check
 * @returns True if valid API key index
 */
export function isValidApiKeyIndex(value: number): boolean {
  return Number.isInteger(value) && value >= 0 && value <= MaxApiKeyIndex;
}

/**
 * Check if value is a valid nonce
 *
 * @param value - Value to check
 * @returns True if valid nonce
 */
export function isValidNonce(value: bigint): boolean {
  return value >= 0n && value <= BigInt(MaxNonce);
}
