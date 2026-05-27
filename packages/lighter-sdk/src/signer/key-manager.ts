/**
 * Key Management Utilities
 *
 * This module provides utilities for generating and managing cryptographic keys
 * for the Lighter protocol. It uses Schnorr signatures over the ECgFp5 curve
 * via the Poseidon crypto library.
 *
 * Key Format:
 * - Private keys: 40 bytes (80 hex characters)
 * - Public keys: 40 bytes (80 hex characters)
 *
 * @module key-manager
 */

import { getCryptoApi, initWasm, isWasmReady, type WasmError } from '@tealstreet/poseidon-crypto';

/**
 * Result of key generation or derivation
 */
export interface KeyPair {
  /** Private key as hex string (80 characters = 40 bytes) */
  privateKey: string;
  /** Public key as hex string (80 characters = 40 bytes) */
  publicKey: string;
}

/**
 * Error thrown during key operations
 */
export class KeyManagerError extends Error {
  constructor(
    message: string,
    public readonly cause?: unknown,
  ) {
    super(message);
    this.name = 'KeyManagerError';
  }
}

/**
 * Ensure WASM is initialized before performing crypto operations.
 *
 * This function should be called before any key generation or derivation.
 * It's safe to call multiple times - initialization only happens once.
 *
 * @throws {KeyManagerError} If WASM initialization fails
 */
export async function ensureWasmInitialized(): Promise<void> {
  if (isWasmReady()) {
    return;
  }

  try {
    await initWasm();
  } catch (error) {
    throw new KeyManagerError('Failed to initialize WASM crypto module', error);
  }

  if (!isWasmReady()) {
    throw new KeyManagerError('WASM initialization completed but module is not ready');
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
 * Generate a new Schnorr key pair.
 *
 * This creates a cryptographically secure random private key and derives
 * the corresponding public key using the ECgFp5 curve.
 *
 * @returns Promise resolving to a new key pair
 * @throws {KeyManagerError} If key generation fails or WASM is not initialized
 *
 * @example
 * ```typescript
 * const keys = await generateKeyPair();
 * console.log('Private key:', keys.privateKey); // 80 hex chars
 * console.log('Public key:', keys.publicKey);   // 80 hex chars
 *
 * // Store the private key securely (e.g., encrypted in a database)
 * // Use the public key for API registration
 * ```
 */
export async function generateKeyPair(): Promise<KeyPair> {
  await ensureWasmInitialized();

  const crypto = getCryptoApi();
  const result = crypto.schnorrGenerateKeys();

  if (isWasmError(result)) {
    throw new KeyManagerError(`Key generation failed: ${result.error}`);
  }

  return {
    privateKey: result.privateKey,
    publicKey: result.publicKey,
  };
}

/**
 * Derive a public key from a private key.
 *
 * This is useful when you have a private key (e.g., loaded from storage)
 * and need to obtain the corresponding public key.
 *
 * @param privateKey - Private key as hex string (80 characters)
 * @returns Promise resolving to the derived public key
 * @throws {KeyManagerError} If derivation fails or WASM is not initialized
 *
 * @example
 * ```typescript
 * const privateKey = "1234..."; // 80 hex characters
 * const publicKey = await derivePublicKey(privateKey);
 * console.log('Derived public key:', publicKey);
 * ```
 */
export async function derivePublicKey(privateKey: string): Promise<string> {
  await ensureWasmInitialized();

  // Validate private key format
  if (!isValidPrivateKey(privateKey)) {
    throw new KeyManagerError(`Invalid private key format. Expected 80 hex characters, got ${privateKey.length}`);
  }

  const crypto = getCryptoApi();
  const result = crypto.schnorrPkFromSk(privateKey);

  if (isWasmError(result)) {
    throw new KeyManagerError(`Public key derivation failed: ${result.error}`);
  }

  return result;
}

/**
 * Validate that a string is a valid private key format.
 *
 * A valid private key is a hex string of exactly 80 characters (40 bytes).
 *
 * @param privateKey - String to validate
 * @returns True if the string is a valid private key format
 *
 * @example
 * ```typescript
 * isValidPrivateKey("1234567890abcdef..."); // 80 chars -> true
 * isValidPrivateKey("abc123");              // too short -> false
 * isValidPrivateKey("xyz...");              // invalid hex -> false
 * ```
 */
export function isValidPrivateKey(privateKey: string): boolean {
  // Must be exactly 80 hex characters (40 bytes)
  if (privateKey.length !== 80) {
    return false;
  }

  // Must contain only hexadecimal characters
  return /^[0-9a-fA-F]{80}$/.test(privateKey);
}

/**
 * Validate that a string is a valid public key format.
 *
 * A valid public key is a hex string of exactly 80 characters (40 bytes).
 *
 * @param publicKey - String to validate
 * @returns True if the string is a valid public key format
 *
 * @example
 * ```typescript
 * isValidPublicKey("abcdef1234567890..."); // 80 chars -> true
 * isValidPublicKey("abc");                 // too short -> false
 * ```
 */
export function isValidPublicKey(publicKey: string): boolean {
  // Must be exactly 80 hex characters (40 bytes)
  if (publicKey.length !== 80) {
    return false;
  }

  // Must contain only hexadecimal characters
  return /^[0-9a-fA-F]{80}$/.test(publicKey);
}

/**
 * Convert a private key from various formats to the canonical hex format.
 *
 * This handles:
 * - Hex strings with or without '0x' prefix
 * - Buffer objects
 * - Uint8Array
 *
 * @param key - Private key in various formats
 * @returns Canonical hex string (80 characters, no prefix)
 * @throws {KeyManagerError} If the key cannot be converted to valid format
 *
 * @example
 * ```typescript
 * normalizePrivateKey("0x1234..."); // Remove 0x prefix
 * normalizePrivateKey(buffer);      // Convert Buffer to hex
 * normalizePrivateKey(uint8Array);  // Convert Uint8Array to hex
 * ```
 */
export function normalizePrivateKey(key: string | Buffer | Uint8Array): string {
  let hexKey: string;

  if (typeof key === 'string') {
    // Remove 0x prefix if present
    hexKey = key.startsWith('0x') ? key.slice(2) : key;
  } else if (Buffer.isBuffer(key)) {
    // Convert Buffer to hex
    hexKey = key.toString('hex');
  } else if (key instanceof Uint8Array) {
    // Convert Uint8Array to hex
    hexKey = Array.from(key)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  } else {
    throw new KeyManagerError('Private key must be a string, Buffer, or Uint8Array');
  }

  // Validate the normalized key
  if (!isValidPrivateKey(hexKey)) {
    throw new KeyManagerError(
      `Invalid private key format after normalization. Expected 80 hex characters, got ${hexKey.length}`,
    );
  }

  return hexKey;
}

/**
 * Convert a public key from various formats to the canonical hex format.
 *
 * This handles:
 * - Hex strings with or without '0x' prefix
 * - Buffer objects
 * - Uint8Array
 *
 * @param key - Public key in various formats
 * @returns Canonical hex string (80 characters, no prefix)
 * @throws {KeyManagerError} If the key cannot be converted to valid format
 *
 * @example
 * ```typescript
 * normalizePublicKey("0xabcd..."); // Remove 0x prefix
 * normalizePublicKey(buffer);      // Convert Buffer to hex
 * normalizePublicKey(uint8Array);  // Convert Uint8Array to hex
 * ```
 */
export function normalizePublicKey(key: string | Buffer | Uint8Array): string {
  let hexKey: string;

  if (typeof key === 'string') {
    // Remove 0x prefix if present
    hexKey = key.startsWith('0x') ? key.slice(2) : key;
  } else if (Buffer.isBuffer(key)) {
    // Convert Buffer to hex
    hexKey = key.toString('hex');
  } else if (key instanceof Uint8Array) {
    // Convert Uint8Array to hex
    hexKey = Array.from(key)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  } else {
    throw new KeyManagerError('Public key must be a string, Buffer, or Uint8Array');
  }

  // Validate the normalized key
  if (!isValidPublicKey(hexKey)) {
    throw new KeyManagerError(
      `Invalid public key format after normalization. Expected 80 hex characters, got ${hexKey.length}`,
    );
  }

  return hexKey;
}

/**
 * Securely compare two private keys in constant time.
 *
 * This prevents timing attacks when comparing keys.
 *
 * @param key1 - First private key
 * @param key2 - Second private key
 * @returns True if the keys are identical
 *
 * @example
 * ```typescript
 * if (secureCompareKeys(storedKey, providedKey)) {
 *   console.log('Keys match');
 * }
 * ```
 */
export function secureCompareKeys(key1: string, key2: string): boolean {
  if (key1.length !== key2.length) {
    return false;
  }

  let result = 0;
  for (let i = 0; i < key1.length; i++) {
    result |= key1.charCodeAt(i) ^ key2.charCodeAt(i);
  }

  return result === 0;
}