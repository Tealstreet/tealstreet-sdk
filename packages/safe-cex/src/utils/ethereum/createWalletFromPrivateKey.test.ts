import { describe, it, expect } from 'vitest';
import { createWalletFromPrivateKey } from './createWalletFromPrivateKey';

// Test private key (DO NOT use in production - this is a well-known test key)
// This key is commonly used in testing and has no real value
const TEST_PRIVATE_KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
const TEST_ADDRESS = '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266';

describe('createWalletFromPrivateKey', () => {
  describe('valid private keys', () => {
    it('should create a wallet from a valid private key', () => {
      const wallet = createWalletFromPrivateKey(TEST_PRIVATE_KEY);
      expect(wallet).toBeDefined();
      expect(wallet.address).toBeDefined();
    });

    it('should derive correct address from private key', () => {
      const wallet = createWalletFromPrivateKey(TEST_PRIVATE_KEY);
      expect(wallet.address.toLowerCase()).toBe(TEST_ADDRESS.toLowerCase());
    });

    it('should handle private key without 0x prefix', () => {
      const keyWithoutPrefix = TEST_PRIVATE_KEY.slice(2);
      const wallet = createWalletFromPrivateKey(keyWithoutPrefix);
      expect(wallet.address.toLowerCase()).toBe(TEST_ADDRESS.toLowerCase());
    });
  });

  describe('wallet properties', () => {
    it('should have privateKey property', () => {
      const wallet = createWalletFromPrivateKey(TEST_PRIVATE_KEY);
      expect(wallet.privateKey).toBeDefined();
      expect(wallet.privateKey).toBe(TEST_PRIVATE_KEY);
    });

    it('should have signMessage method', () => {
      const wallet = createWalletFromPrivateKey(TEST_PRIVATE_KEY);
      expect(typeof wallet.signMessage).toBe('function');
    });

    it('should have signTransaction method', () => {
      const wallet = createWalletFromPrivateKey(TEST_PRIVATE_KEY);
      expect(typeof wallet.signTransaction).toBe('function');
    });
  });

  describe('error cases', () => {
    it('should throw for invalid private key', () => {
      expect(() => {
        createWalletFromPrivateKey('invalid-key');
      }).toThrow();
    });

    it('should throw for empty string', () => {
      expect(() => {
        createWalletFromPrivateKey('');
      }).toThrow();
    });

    it('should throw for private key too short', () => {
      expect(() => {
        createWalletFromPrivateKey('0x1234');
      }).toThrow();
    });

    it('should throw for private key too long', () => {
      expect(() => {
        createWalletFromPrivateKey(TEST_PRIVATE_KEY + 'abcd');
      }).toThrow();
    });
  });
});
