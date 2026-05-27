/**
 * Tests for key management utilities
 */

import { describe, expect, it, beforeAll } from 'vitest';

import {
  derivePublicKey,
  ensureWasmInitialized,
  generateKeyPair,
  isValidPrivateKey,
  isValidPublicKey,
  KeyManagerError,
  normalizePrivateKey,
  normalizePublicKey,
  secureCompareKeys,
} from './key-manager';

describe('Key Manager', () => {
  // Ensure WASM is initialized before all tests
  beforeAll(async () => {
    await ensureWasmInitialized();
  });

  describe('ensureWasmInitialized', () => {
    it('should initialize WASM successfully', async () => {
      await expect(ensureWasmInitialized()).resolves.toBeUndefined();
    });

    it('should be idempotent (safe to call multiple times)', async () => {
      await ensureWasmInitialized();
      await ensureWasmInitialized();
      await ensureWasmInitialized();
    });
  });

  describe('generateKeyPair', () => {
    it('should generate a valid key pair', async () => {
      const keys = await generateKeyPair();

      expect(keys).toBeDefined();
      expect(keys.privateKey).toBeDefined();
      expect(keys.publicKey).toBeDefined();
      expect(keys.privateKey.length).toBe(80); // 40 bytes = 80 hex chars
      expect(keys.publicKey.length).toBe(80); // 40 bytes = 80 hex chars
    });

    it('should generate different key pairs each time', async () => {
      const keys1 = await generateKeyPair();
      const keys2 = await generateKeyPair();

      expect(keys1.privateKey).not.toBe(keys2.privateKey);
      expect(keys1.publicKey).not.toBe(keys2.publicKey);
    });

    it('should generate valid hex strings', async () => {
      const keys = await generateKeyPair();

      expect(/^[0-9a-f]{80}$/i.test(keys.privateKey)).toBe(true);
      expect(/^[0-9a-f]{80}$/i.test(keys.publicKey)).toBe(true);
    });
  });

  describe('derivePublicKey', () => {
    it('should derive public key from private key', async () => {
      const keys = await generateKeyPair();
      const derivedPublicKey = await derivePublicKey(keys.privateKey);

      expect(derivedPublicKey).toBe(keys.publicKey);
    });

    it('should consistently derive the same public key', async () => {
      const keys = await generateKeyPair();
      const derived1 = await derivePublicKey(keys.privateKey);
      const derived2 = await derivePublicKey(keys.privateKey);

      expect(derived1).toBe(derived2);
    });

    it('should throw error for invalid private key format', async () => {
      await expect(derivePublicKey('invalid')).rejects.toThrow(KeyManagerError);
      await expect(derivePublicKey('12345')).rejects.toThrow(KeyManagerError);
    });

    it('should throw error for private key with wrong length', async () => {
      const shortKey = 'a'.repeat(79);
      const longKey = 'a'.repeat(81);

      await expect(derivePublicKey(shortKey)).rejects.toThrow(KeyManagerError);
      await expect(derivePublicKey(longKey)).rejects.toThrow(KeyManagerError);
    });
  });

  describe('isValidPrivateKey', () => {
    it('should validate correct private keys', () => {
      const validKey = 'a'.repeat(80);
      expect(isValidPrivateKey(validKey)).toBe(true);
    });

    it('should accept mixed case hex characters', () => {
      const validKey = 'A'.repeat(40) + 'a'.repeat(40);
      expect(isValidPrivateKey(validKey)).toBe(true);
    });

    it('should accept numeric hex characters', () => {
      const validKey = '0123456789abcdef'.repeat(5);
      expect(isValidPrivateKey(validKey)).toBe(true);
    });

    it('should reject keys with wrong length', () => {
      expect(isValidPrivateKey('a'.repeat(79))).toBe(false);
      expect(isValidPrivateKey('a'.repeat(81))).toBe(false);
      expect(isValidPrivateKey('')).toBe(false);
    });

    it('should reject keys with non-hex characters', () => {
      const invalidKey = 'g'.repeat(80);
      expect(isValidPrivateKey(invalidKey)).toBe(false);

      const keyWithSpace = 'a'.repeat(79) + ' ';
      expect(isValidPrivateKey(keyWithSpace)).toBe(false);
    });

    it('should reject keys with 0x prefix', () => {
      const keyWith0x = '0x' + 'a'.repeat(78);
      expect(isValidPrivateKey(keyWith0x)).toBe(false);
    });
  });

  describe('isValidPublicKey', () => {
    it('should validate correct public keys', () => {
      const validKey = 'b'.repeat(80);
      expect(isValidPublicKey(validKey)).toBe(true);
    });

    it('should accept mixed case hex characters', () => {
      const validKey = 'B'.repeat(40) + 'b'.repeat(40);
      expect(isValidPublicKey(validKey)).toBe(true);
    });

    it('should reject keys with wrong length', () => {
      expect(isValidPublicKey('b'.repeat(79))).toBe(false);
      expect(isValidPublicKey('b'.repeat(81))).toBe(false);
      expect(isValidPublicKey('')).toBe(false);
    });

    it('should reject keys with non-hex characters', () => {
      const invalidKey = 'xyz' + 'b'.repeat(77);
      expect(isValidPublicKey(invalidKey)).toBe(false);
    });
  });

  describe('normalizePrivateKey', () => {
    it('should handle hex strings without 0x prefix', () => {
      const key = 'a'.repeat(80);
      expect(normalizePrivateKey(key)).toBe(key);
    });

    it('should remove 0x prefix', () => {
      const keyWithout0x = 'a'.repeat(80);
      const keyWith0x = '0x' + keyWithout0x;
      expect(normalizePrivateKey(keyWith0x)).toBe(keyWithout0x);
    });

    it('should convert Buffer to hex string', () => {
      // Create a 40-byte buffer (80 hex chars)
      const buffer = Buffer.alloc(40, 0xaa);
      const result = normalizePrivateKey(buffer);
      expect(result).toHaveLength(80);
      expect(result).toBe('aa'.repeat(40));
      expect(/^[0-9a-f]{80}$/i.test(result)).toBe(true);
    });

    it('should convert Uint8Array to hex string', () => {
      const uint8Array = new Uint8Array(40).fill(0xaa);
      const result = normalizePrivateKey(uint8Array);
      expect(result).toHaveLength(80);
      expect(result).toBe('aa'.repeat(40));
    });

    it('should throw error for invalid input types', () => {
      expect(() => normalizePrivateKey(123 as any)).toThrow(KeyManagerError);
      expect(() => normalizePrivateKey({} as any)).toThrow(KeyManagerError);
      expect(() => normalizePrivateKey(null as any)).toThrow(KeyManagerError);
    });

    it('should throw error for invalid length after normalization', () => {
      expect(() => normalizePrivateKey('a'.repeat(79))).toThrow(KeyManagerError);
      expect(() => normalizePrivateKey('0x' + 'a'.repeat(79))).toThrow(KeyManagerError);
    });
  });

  describe('normalizePublicKey', () => {
    it('should handle hex strings without 0x prefix', () => {
      const key = 'b'.repeat(80);
      expect(normalizePublicKey(key)).toBe(key);
    });

    it('should remove 0x prefix', () => {
      const keyWithout0x = 'b'.repeat(80);
      const keyWith0x = '0x' + keyWithout0x;
      expect(normalizePublicKey(keyWith0x)).toBe(keyWithout0x);
    });

    it('should convert Buffer to hex string', () => {
      // Create a 40-byte buffer (80 hex chars)
      const buffer = Buffer.alloc(40, 0xbb);
      const result = normalizePublicKey(buffer);
      expect(result).toHaveLength(80);
      expect(result).toBe('bb'.repeat(40));
      expect(/^[0-9a-f]{80}$/i.test(result)).toBe(true);
    });

    it('should convert Uint8Array to hex string', () => {
      const uint8Array = new Uint8Array(40).fill(0xbb);
      const result = normalizePublicKey(uint8Array);
      expect(result).toHaveLength(80);
      expect(result).toBe('bb'.repeat(40));
    });

    it('should throw error for invalid input types', () => {
      expect(() => normalizePublicKey(456 as any)).toThrow(KeyManagerError);
      expect(() => normalizePublicKey([] as any)).toThrow(KeyManagerError);
    });

    it('should throw error for invalid length after normalization', () => {
      expect(() => normalizePublicKey('b'.repeat(79))).toThrow(KeyManagerError);
      expect(() => normalizePublicKey('0x' + 'b'.repeat(79))).toThrow(KeyManagerError);
    });
  });

  describe('secureCompareKeys', () => {
    it('should return true for identical keys', () => {
      const key1 = 'a'.repeat(80);
      const key2 = 'a'.repeat(80);
      expect(secureCompareKeys(key1, key2)).toBe(true);
    });

    it('should return false for different keys', () => {
      const key1 = 'a'.repeat(80);
      const key2 = 'b'.repeat(80);
      expect(secureCompareKeys(key1, key2)).toBe(false);
    });

    it('should return false for keys with different lengths', () => {
      const key1 = 'a'.repeat(80);
      const key2 = 'a'.repeat(79);
      expect(secureCompareKeys(key1, key2)).toBe(false);
    });

    it('should detect single character difference', () => {
      const key1 = 'a'.repeat(80);
      const key2 = 'a'.repeat(79) + 'b';
      expect(secureCompareKeys(key1, key2)).toBe(false);
    });

    it('should handle empty strings', () => {
      expect(secureCompareKeys('', '')).toBe(true);
      expect(secureCompareKeys('', 'a')).toBe(false);
    });

    it('should be case-sensitive', () => {
      const key1 = 'a'.repeat(80);
      const key2 = 'A'.repeat(80);
      expect(secureCompareKeys(key1, key2)).toBe(false);
    });
  });

  describe('integration tests', () => {
    it('should generate and derive keys correctly', async () => {
      const keys = await generateKeyPair();

      expect(isValidPrivateKey(keys.privateKey)).toBe(true);
      expect(isValidPublicKey(keys.publicKey)).toBe(true);

      const derivedPublicKey = await derivePublicKey(keys.privateKey);
      expect(derivedPublicKey).toBe(keys.publicKey);
    });

    it('should normalize and validate keys in a workflow', async () => {
      const keys = await generateKeyPair();

      // Add 0x prefix
      const keyWith0x = '0x' + keys.privateKey;

      // Normalize should remove it
      const normalized = normalizePrivateKey(keyWith0x);

      // Should match original
      expect(normalized).toBe(keys.privateKey);

      // Should be valid
      expect(isValidPrivateKey(normalized)).toBe(true);

      // Should derive same public key
      const derivedPublicKey = await derivePublicKey(normalized);
      expect(derivedPublicKey).toBe(keys.publicKey);
    });

    it('should handle Buffer conversion workflow', async () => {
      const keys = await generateKeyPair();

      // Convert to buffer
      const buffer = Buffer.from(keys.privateKey, 'hex');

      // Normalize from buffer
      const normalized = normalizePrivateKey(buffer);

      // Should match original
      expect(normalized).toBe(keys.privateKey);

      // Should derive same public key
      const derivedPublicKey = await derivePublicKey(normalized);
      expect(derivedPublicKey).toBe(keys.publicKey);
    });
  });
});
