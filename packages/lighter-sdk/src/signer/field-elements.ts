/**
 * Field Element Conversion Utilities
 *
 * This module provides utilities for converting JavaScript numbers and BigInts
 * into field elements suitable for Poseidon hash-based cryptographic operations.
 *
 * Field elements are 8-byte (64-bit) values encoded as hexadecimal strings.
 * All conversions use little-endian byte order to match the Lighter protocol specification.
 *
 * @module field-elements
 */

/**
 * Convert a uint32 value to a field element (8-byte hex string).
 *
 * A uint32 is a 32-bit unsigned integer (4 bytes). This function pads it to
 * 8 bytes using little-endian format.
 *
 * @param value - The uint32 value to convert (0 to 2^32-1)
 * @returns Hex string representing the field element (16 hex characters = 8 bytes)
 * @throws Error if value is negative or exceeds uint32 range
 *
 * @example
 * ```typescript
 * uint32ToFieldElement(42)
 * // Returns: "2a00000000000000" (42 in little-endian, padded to 8 bytes)
 * ```
 */
export function uint32ToFieldElement(value: number): string {
  if (value < 0 || value > 0xffffffff) {
    throw new Error(`Value ${value} out of range for uint32 (0 to ${0xffffffff})`);
  }

  // Convert to 8-byte buffer in little-endian format
  const buffer = new ArrayBuffer(8);
  const view = new DataView(buffer);
  view.setUint32(0, value, true); // true = little-endian

  // Convert buffer to hex string
  const bytes = new Uint8Array(buffer);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Convert a uint64 value to a field element (8-byte hex string).
 *
 * A uint64 is a 64-bit unsigned integer (8 bytes) represented as a BigInt.
 *
 * @param value - The uint64 value to convert as BigInt (0 to 2^64-1)
 * @returns Hex string representing the field element (16 hex characters = 8 bytes)
 * @throws Error if value is negative or exceeds uint64 range
 *
 * @example
 * ```typescript
 * uint64ToFieldElement(1000000000000n)
 * // Returns: "00e8764817000000" (1 trillion in little-endian)
 * ```
 */
export function uint64ToFieldElement(value: bigint): string {
  const MAX_UINT64 = (1n << 64n) - 1n;

  if (value < 0n || value > MAX_UINT64) {
    throw new Error(`Value ${value} out of range for uint64 (0 to ${MAX_UINT64})`);
  }

  // Convert to 8-byte buffer in little-endian format
  const buffer = new ArrayBuffer(8);
  const view = new DataView(buffer);
  view.setBigUint64(0, value, true); // true = little-endian

  // Convert buffer to hex string
  const bytes = new Uint8Array(buffer);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Convert an int64 value to a field element (8-byte hex string).
 *
 * An int64 is a 64-bit signed integer (8 bytes) represented as a BigInt.
 * This uses two's complement representation for negative values.
 *
 * @param value - The int64 value to convert as BigInt (-2^63 to 2^63-1)
 * @returns Hex string representing the field element (16 hex characters = 8 bytes)
 * @throws Error if value exceeds int64 range
 *
 * @example
 * ```typescript
 * int64ToFieldElement(-100n)
 * // Returns: "9cffffffffffffff" (-100 in two's complement, little-endian)
 * ```
 */
export function int64ToFieldElement(value: bigint): string {
  const MIN_INT64 = -(1n << 63n);
  const MAX_INT64 = (1n << 63n) - 1n;

  if (value < MIN_INT64 || value > MAX_INT64) {
    throw new Error(`Value ${value} out of range for int64 (${MIN_INT64} to ${MAX_INT64})`);
  }

  // Convert to 8-byte buffer in little-endian format
  const buffer = new ArrayBuffer(8);
  const view = new DataView(buffer);
  view.setBigInt64(0, value, true); // true = little-endian

  // Convert buffer to hex string
  const bytes = new Uint8Array(buffer);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Split a uint64 value into two uint32 field elements.
 *
 * This is useful when a protocol requires splitting a 64-bit value into
 * two 32-bit field elements for hashing or signing.
 *
 * @param value - The uint64 value to split as BigInt
 * @returns Tuple of [low32, high32] as field element hex strings
 * @throws Error if value is negative or exceeds uint64 range
 *
 * @example
 * ```typescript
 * splitUint64(0x123456789ABCDEFn)
 * // Returns: ["efcdab8900000000", "2345670100000000"]
 * // low32 = 0x89ABCDEF, high32 = 0x01234567
 * ```
 */
export function splitUint64(value: bigint): [string, string] {
  const MAX_UINT64 = (1n << 64n) - 1n;

  if (value < 0n || value > MAX_UINT64) {
    throw new Error(`Value ${value} out of range for uint64 (0 to ${MAX_UINT64})`);
  }

  // Extract low and high 32 bits
  const low32 = Number(value & 0xffffffffn);
  const high32 = Number((value >> 32n) & 0xffffffffn);

  return [uint32ToFieldElement(low32), uint32ToFieldElement(high32)];
}

/**
 * Convert a hex string to field element format (ensure 8-byte alignment).
 *
 * Poseidon hash requires inputs to be multiples of 8 bytes (16 hex characters).
 * This function pads the input to the nearest 8-byte boundary.
 *
 * @param hex - Hex string (with or without '0x' prefix)
 * @returns Padded hex string (multiple of 16 hex characters)
 *
 * @example
 * ```typescript
 * hexToFieldElement("0x1234")
 * // Returns: "3412000000000000" (little-endian, padded to 8 bytes)
 * ```
 */
export function hexToFieldElement(hex: string): string {
  // Remove 0x prefix if present
  const cleanHex = hex.startsWith('0x') ? hex.slice(2) : hex;

  // Ensure even number of characters
  const paddedHex = cleanHex.length % 2 === 0 ? cleanHex : '0' + cleanHex;

  // Convert to bytes
  const bytes: number[] = [];
  for (let i = 0; i < paddedHex.length; i += 2) {
    bytes.push(parseInt(paddedHex.slice(i, i + 2), 16));
  }

  // Pad to 8-byte boundary if needed
  while (bytes.length % 8 !== 0) {
    bytes.push(0);
  }

  // Convert to hex string
  return bytes.map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Concatenate multiple field elements into a single hex string for hashing.
 *
 * This is used to build the message data before passing it to Poseidon hash.
 * Each field element must be exactly 8 bytes (16 hex characters).
 *
 * @param elements - Array of field element hex strings
 * @returns Concatenated hex string
 * @throws Error if any element is not exactly 16 hex characters
 *
 * @example
 * ```typescript
 * concatenateFieldElements([
 *   "0100000000000000",
 *   "0200000000000000",
 *   "0300000000000000"
 * ])
 * // Returns: "010000000000000002000000000000000300000000000000"
 * ```
 */
export function concatenateFieldElements(elements: string[]): string {
  for (let i = 0; i < elements.length; i++) {
    if (elements[i].length !== 16) {
      throw new Error(
        `Field element at index ${i} has invalid length ${elements[i].length} (expected 16 hex chars)`,
      );
    }
  }

  return elements.join('');
}
