import { describe, it, expect } from 'vitest';
import {
  timestamp,
  assert,
  assertTypeArray,
  assertTypeBoolean,
  assertTypeNumber,
  assertTypeString,
  assertAddress,
  assertChainId,
  assertAccountId,
  assertSubAccountId,
  assertNonce,
  assertTokenId,
  assertPubKeyHash,
  assertTs,
  assertWithdrawFeeRatio,
  assertTradingFeeRatio,
} from './utils';

describe('zklink signer utils', () => {
  describe('timestamp', () => {
    it('should return current unix timestamp in seconds', () => {
      const ts = timestamp();
      const expectedTs = Math.floor(Date.now() / 1000);
      // Allow 1 second tolerance
      expect(ts).toBeGreaterThanOrEqual(expectedTs - 1);
      expect(ts).toBeLessThanOrEqual(expectedTs + 1);
    });

    it('should return a 10-digit number', () => {
      const ts = timestamp();
      expect(String(ts).length).toBe(10);
    });
  });

  describe('assert', () => {
    it('should not throw when condition is true', () => {
      expect(() => assert(true, 'should not throw')).not.toThrow();
    });

    it('should throw when condition is false', () => {
      expect(() => assert(false, 'test error')).toThrow('test error');
    });
  });

  describe('assertTypeArray', () => {
    it('should not throw for arrays', () => {
      expect(() => assertTypeArray([], 'test')).not.toThrow();
      expect(() => assertTypeArray([1, 2, 3], 'test')).not.toThrow();
    });

    it('should throw for non-arrays', () => {
      expect(() => assertTypeArray('string', 'test')).toThrow();
      expect(() => assertTypeArray(123, 'test')).toThrow();
      expect(() => assertTypeArray({}, 'test')).toThrow();
    });
  });

  describe('assertTypeBoolean', () => {
    it('should not throw for booleans', () => {
      expect(() => assertTypeBoolean(true, 'test')).not.toThrow();
      expect(() => assertTypeBoolean(false, 'test')).not.toThrow();
    });

    it('should throw for non-booleans', () => {
      expect(() => assertTypeBoolean('true', 'test')).toThrow();
      expect(() => assertTypeBoolean(1, 'test')).toThrow();
      expect(() => assertTypeBoolean(null, 'test')).toThrow();
    });
  });

  describe('assertTypeNumber', () => {
    it('should not throw for numbers', () => {
      expect(() => assertTypeNumber(123, 'test')).not.toThrow();
      expect(() => assertTypeNumber(0, 'test')).not.toThrow();
      expect(() => assertTypeNumber(-1, 'test')).not.toThrow();
      expect(() => assertTypeNumber(1.5, 'test')).not.toThrow();
    });

    it('should throw for non-numbers', () => {
      expect(() => assertTypeNumber('123', 'test')).toThrow();
      expect(() => assertTypeNumber(null, 'test')).toThrow();
    });
  });

  describe('assertTypeString', () => {
    it('should not throw for strings', () => {
      expect(() => assertTypeString('test', 'test')).not.toThrow();
      expect(() => assertTypeString('', 'test')).not.toThrow();
    });

    it('should throw for non-strings', () => {
      expect(() => assertTypeString(123, 'test')).toThrow();
      expect(() => assertTypeString(null, 'test')).toThrow();
    });
  });

  describe('assertAddress', () => {
    it('should not throw for valid Ethereum addresses', () => {
      // All lowercase is valid
      expect(() => assertAddress('0x0000000000000000000000000000000000000000', 'test')).not.toThrow();
      // Properly checksummed address
      expect(() => assertAddress('0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045', 'test')).not.toThrow();
    });

    it('should throw for invalid addresses', () => {
      expect(() => assertAddress('0x123', 'test')).toThrow();
      expect(() => assertAddress('invalid', 'test')).toThrow();
      expect(() => assertAddress('', 'test')).toThrow();
    });
  });

  describe('assertChainId', () => {
    it('should not throw for valid chain IDs', () => {
      expect(() => assertChainId(1, 'test')).not.toThrow();
      expect(() => assertChainId(42161, 'test')).not.toThrow(); // Arbitrum
    });

    it('should throw for invalid chain IDs', () => {
      expect(() => assertChainId(0, 'test')).toThrow();
      expect(() => assertChainId(-1, 'test')).toThrow();
    });
  });

  describe('assertAccountId', () => {
    it('should not throw for valid account IDs', () => {
      expect(() => assertAccountId(1, 'test')).not.toThrow();
      expect(() => assertAccountId(100000, 'test')).not.toThrow();
    });

    it('should throw for invalid account IDs', () => {
      expect(() => assertAccountId(0, 'test')).toThrow();
      expect(() => assertAccountId(-1, 'test')).toThrow();
    });
  });

  describe('assertSubAccountId', () => {
    it('should not throw (noop)', () => {
      expect(() => assertSubAccountId(0, 'test')).not.toThrow();
      expect(() => assertSubAccountId(1, 'test')).not.toThrow();
    });
  });

  describe('assertNonce', () => {
    it('should not throw for valid nonces', () => {
      expect(() => assertNonce(0, 'test')).not.toThrow();
      expect(() => assertNonce(1, 'test')).not.toThrow();
      expect(() => assertNonce(1000000, 'test')).not.toThrow();
    });

    it('should throw for invalid nonces', () => {
      expect(() => assertNonce(-1, 'test')).toThrow();
    });
  });

  describe('assertTokenId', () => {
    it('should not throw for valid token IDs', () => {
      expect(() => assertTokenId(1, 'test')).not.toThrow();
      expect(() => assertTokenId(100, 'test')).not.toThrow();
    });

    it('should throw for invalid token IDs', () => {
      expect(() => assertTokenId(0, 'test')).toThrow();
      expect(() => assertTokenId(-1, 'test')).toThrow();
    });
  });

  describe('assertPubKeyHash', () => {
    it('should not throw for valid pubkey hashes', () => {
      const validHash = '0x' + '1'.repeat(40);
      expect(() => assertPubKeyHash(validHash, 'test')).not.toThrow();
    });

    it('should throw for invalid pubkey hashes', () => {
      expect(() => assertPubKeyHash('0x123', 'test')).toThrow();
      expect(() => assertPubKeyHash('invalid', 'test')).toThrow();
      expect(() => assertPubKeyHash('', 'test')).toThrow();
    });
  });

  describe('assertTs', () => {
    it('should not throw for valid timestamps', () => {
      expect(() => assertTs(1700000001, 'test')).not.toThrow();
      expect(() => assertTs(1800000000, 'test')).not.toThrow();
    });

    it('should throw for invalid timestamps', () => {
      expect(() => assertTs(1600000000, 'test')).toThrow(); // Too old
      expect(() => assertTs(0, 'test')).toThrow();
    });
  });

  describe('assertWithdrawFeeRatio', () => {
    it('should not throw for valid fee ratios', () => {
      expect(() => assertWithdrawFeeRatio(0, 'test')).not.toThrow();
      expect(() => assertWithdrawFeeRatio(100, 'test')).not.toThrow();
      expect(() => assertWithdrawFeeRatio(10000, 'test')).not.toThrow();
    });

    it('should throw for invalid fee ratios', () => {
      expect(() => assertWithdrawFeeRatio(-1, 'test')).toThrow();
      expect(() => assertWithdrawFeeRatio(10001, 'test')).toThrow();
    });
  });

  describe('assertTradingFeeRatio', () => {
    it('should not throw for valid fee ratios', () => {
      expect(() => assertTradingFeeRatio(0, 'test')).not.toThrow();
      expect(() => assertTradingFeeRatio(255, 'test')).not.toThrow();
      expect(() => assertTradingFeeRatio(-255, 'test')).not.toThrow();
    });

    it('should throw for invalid fee ratios', () => {
      expect(() => assertTradingFeeRatio(-256, 'test')).toThrow();
      expect(() => assertTradingFeeRatio(256, 'test')).toThrow();
    });
  });
});
