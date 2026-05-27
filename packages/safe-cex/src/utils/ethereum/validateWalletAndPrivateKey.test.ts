import { describe, it, expect } from 'vitest';
import { validateWalletAndPrivateKey } from './validateWalletAndPrivateKey';

// Test private key (DO NOT use in production - this is a well-known test key)
// This key is commonly used in testing and has no real value
const TEST_PRIVATE_KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
const TEST_ADDRESS = '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266';

// Another test key pair
const TEST_PRIVATE_KEY_2 = '0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d';
const TEST_ADDRESS_2 = '0x70997970C51812dc3A010C7d01b50e0d17dc79C8';

describe('validateWalletAndPrivateKey', () => {
  describe('valid key-address pairs', () => {
    it('should return true for matching private key and address', () => {
      const result = validateWalletAndPrivateKey(TEST_PRIVATE_KEY, TEST_ADDRESS);
      expect(result).toBe(true);
    });

    it('should return true for another valid key-address pair', () => {
      const result = validateWalletAndPrivateKey(TEST_PRIVATE_KEY_2, TEST_ADDRESS_2);
      expect(result).toBe(true);
    });

    it('should handle address case insensitively (lowercase)', () => {
      const result = validateWalletAndPrivateKey(
        TEST_PRIVATE_KEY,
        TEST_ADDRESS.toLowerCase(),
      );
      expect(result).toBe(true);
    });

    it('should handle address case insensitively (uppercase)', () => {
      const result = validateWalletAndPrivateKey(
        TEST_PRIVATE_KEY,
        TEST_ADDRESS.toUpperCase(),
      );
      expect(result).toBe(true);
    });
  });

  describe('mismatched key-address pairs', () => {
    it('should return false when private key does not match address', () => {
      const result = validateWalletAndPrivateKey(TEST_PRIVATE_KEY, TEST_ADDRESS_2);
      expect(result).toBe(false);
    });

    it('should return false for swapped key-address pair', () => {
      const result = validateWalletAndPrivateKey(TEST_PRIVATE_KEY_2, TEST_ADDRESS);
      expect(result).toBe(false);
    });
  });

  describe('error cases', () => {
    it('should throw for invalid private key format', () => {
      expect(() => {
        validateWalletAndPrivateKey('invalid-key', TEST_ADDRESS);
      }).toThrow();
    });

    it('should throw for empty private key', () => {
      expect(() => {
        validateWalletAndPrivateKey('', TEST_ADDRESS);
      }).toThrow();
    });

    it('should throw for private key too short', () => {
      expect(() => {
        validateWalletAndPrivateKey('0x1234', TEST_ADDRESS);
      }).toThrow();
    });
  });
});
