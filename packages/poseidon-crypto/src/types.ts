/**
 * Poseidon2 Cryptography Type Definitions
 */

/**
 * Result type for key generation
 */
export interface SchnorrKeyPair {
  privateKey: string; // 80 hex characters (40 bytes)
  publicKey: string; // 80 hex characters (40 bytes)
}

/**
 * Error result from WASM functions
 */
export interface WasmError {
  error: string;
}

/**
 * Poseidon2 hash functions
 */
export interface Poseidon2Hasher {
  /**
   * Hash arbitrary data using Poseidon2
   * @param input - Hex-encoded input data (must be multiple of 8 bytes = 16 hex chars)
   * @returns Hex-encoded hash output (32 bytes = 64 hex chars)
   */
  poseidon2Hash(input: string): string | WasmError;

  /**
   * Hash arbitrary data to Goldilocks quintic extension form.
   * @param input - Hex-encoded input data (must be multiple of 8 bytes = 16 hex chars)
   * @returns Hex-encoded fp5 hash output (40 bytes = 80 hex chars)
   */
  poseidon2HashFp5(input: string): string | WasmError;

  /**
   * Combine two hash outputs into one
   * @param hash1 - First hash (32 bytes = 64 hex chars)
   * @param hash2 - Second hash (32 bytes = 64 hex chars)
   * @returns Combined hash (32 bytes = 64 hex chars)
   */
  poseidon2HashTwoToOne(hash1: string, hash2: string): string | WasmError;
}

/**
 * Schnorr signature functions
 */
export interface SchnorrSigner {
  /**
   * Generate a new Schnorr key pair
   * @returns Object with privateKey and publicKey (both 40 bytes = 80 hex chars)
   */
  schnorrGenerateKeys(): SchnorrKeyPair | WasmError;

  /**
   * Derive public key from private key
   * @param privateKey - Private key (40 bytes = 80 hex chars)
   * @returns Public key (40 bytes = 80 hex chars)
   */
  schnorrPkFromSk(privateKey: string): string | WasmError;

  /**
   * Sign a message
   * @param message - Message to sign as hex (must be multiple of 8 bytes)
   * @param privateKey - Signing key (40 bytes = 80 hex chars)
   * @returns Signature (80 bytes = 160 hex chars)
   */
  schnorrSign(message: string, privateKey: string): string | WasmError;

  /**
   * Verify a signature
   * @param message - Original message as hex
   * @param publicKey - Signer's public key (40 bytes = 80 hex chars)
   * @param signature - Signature to verify (80 bytes = 160 hex chars)
   * @returns true if valid, false otherwise
   */
  schnorrVerify(message: string, publicKey: string, signature: string): boolean | WasmError;
}

/**
 * Combined interface for all Poseidon crypto functions
 */
export interface PoseidonCrypto extends Poseidon2Hasher, SchnorrSigner {
  /**
   * Check if WASM module is loaded and ready
   */
  isReady(): boolean;
}

/**
 * Global WASM functions (available after loading)
 */
declare global {
  function poseidon2Hash(input: string): string | WasmError;
  function poseidon2HashFp5(input: string): string | WasmError;
  function poseidon2HashTwoToOne(hash1: string, hash2: string): string | WasmError;
  function schnorrGenerateKeys(): SchnorrKeyPair | WasmError;
  function schnorrPkFromSk(privateKey: string): string | WasmError;
  function schnorrSign(message: string, privateKey: string): string | WasmError;
  function schnorrVerify(message: string, publicKey: string, signature: string): boolean | WasmError;
}
