/**
 * Tests for field element conversion utilities
 */

import { describe, expect, it } from 'vitest';

import {
  concatenateFieldElements,
  hexToFieldElement,
  int64ToFieldElement,
  splitUint64,
  uint32ToFieldElement,
  uint64ToFieldElement,
} from './field-elements';

describe('Field Element Conversions', () => {
  describe('uint32ToFieldElement', () => {
    it('should convert small values correctly', () => {
      expect(uint32ToFieldElement(0)).toBe('0000000000000000');
      expect(uint32ToFieldElement(1)).toBe('0100000000000000');
      expect(uint32ToFieldElement(42)).toBe('2a00000000000000');
      expect(uint32ToFieldElement(255)).toBe('ff00000000000000');
    });

    it('should convert max uint32 value correctly', () => {
      expect(uint32ToFieldElement(0xffffffff)).toBe('ffffffff00000000');
    });

    it('should handle powers of 2', () => {
      expect(uint32ToFieldElement(256)).toBe('0001000000000000');
      expect(uint32ToFieldElement(65536)).toBe('0000010000000000');
    });

    it('should throw error for negative values', () => {
      expect(() => uint32ToFieldElement(-1)).toThrow('out of range for uint32');
      expect(() => uint32ToFieldElement(-100)).toThrow('out of range for uint32');
    });

    it('should throw error for values exceeding uint32 range', () => {
      expect(() => uint32ToFieldElement(0xffffffff + 1)).toThrow('out of range for uint32');
      expect(() => uint32ToFieldElement(2 ** 32)).toThrow('out of range for uint32');
    });
  });

  describe('uint64ToFieldElement', () => {
    it('should convert small values correctly', () => {
      expect(uint64ToFieldElement(0n)).toBe('0000000000000000');
      expect(uint64ToFieldElement(1n)).toBe('0100000000000000');
      expect(uint64ToFieldElement(255n)).toBe('ff00000000000000');
    });

    it('should convert large values correctly', () => {
      // 1000000000000 = 0xE8D4A51000 in hex
      // In little-endian bytes: 00 10 A5 D4 E8 00 00 00
      expect(uint64ToFieldElement(1000000000000n)).toBe('0010a5d4e8000000');
      expect(uint64ToFieldElement(0xffffffffffffffffn)).toBe('ffffffffffffffff');
    });

    it('should handle values larger than uint32', () => {
      expect(uint64ToFieldElement(0x100000000n)).toBe('0000000001000000');
      expect(uint64ToFieldElement(0x123456789abcdefn)).toBe('efcdab8967452301');
    });

    it('should throw error for negative values', () => {
      expect(() => uint64ToFieldElement(-1n)).toThrow('out of range for uint64');
      expect(() => uint64ToFieldElement(-100n)).toThrow('out of range for uint64');
    });

    it('should throw error for values exceeding uint64 range', () => {
      const maxUint64 = (1n << 64n) - 1n;
      expect(() => uint64ToFieldElement(maxUint64 + 1n)).toThrow('out of range for uint64');
    });
  });

  describe('int64ToFieldElement', () => {
    it('should convert positive values correctly', () => {
      expect(int64ToFieldElement(0n)).toBe('0000000000000000');
      expect(int64ToFieldElement(1n)).toBe('0100000000000000');
      expect(int64ToFieldElement(100n)).toBe('6400000000000000');
    });

    it('should convert negative values in two\'s complement', () => {
      expect(int64ToFieldElement(-1n)).toBe('ffffffffffffffff');
      expect(int64ToFieldElement(-100n)).toBe('9cffffffffffffff');
      expect(int64ToFieldElement(-256n)).toBe('00ffffffffffffff');
    });

    it('should handle max and min int64 values', () => {
      const maxInt64 = (1n << 63n) - 1n;
      const minInt64 = -(1n << 63n);

      expect(int64ToFieldElement(maxInt64)).toBe('ffffffffffffff7f');
      expect(int64ToFieldElement(minInt64)).toBe('0000000000000080');
    });

    it('should throw error for values exceeding int64 range', () => {
      const maxInt64 = (1n << 63n) - 1n;
      const minInt64 = -(1n << 63n);

      expect(() => int64ToFieldElement(maxInt64 + 1n)).toThrow('out of range for int64');
      expect(() => int64ToFieldElement(minInt64 - 1n)).toThrow('out of range for int64');
    });
  });

  describe('splitUint64', () => {
    it('should split zero correctly', () => {
      const [low, high] = splitUint64(0n);
      expect(low).toBe('0000000000000000');
      expect(high).toBe('0000000000000000');
    });

    it('should split values that fit in low 32 bits', () => {
      const [low, high] = splitUint64(0xabcdefn);
      expect(low).toBe('efcdab0000000000');
      expect(high).toBe('0000000000000000');
    });

    it('should split values requiring both parts', () => {
      // 0x0123456789ABCDEF
      // low32 = 0x89ABCDEF, high32 = 0x01234567
      // In little-endian: low = EF CD AB 89, high = 67 45 23 01
      const [low, high] = splitUint64(0x123456789abcdefn);
      expect(low).toBe('efcdab8900000000');
      expect(high).toBe('6745230100000000');
    });

    it('should split max uint64 correctly', () => {
      const [low, high] = splitUint64(0xffffffffffffffffn);
      expect(low).toBe('ffffffff00000000');
      expect(high).toBe('ffffffff00000000');
    });

    it('should throw error for negative values', () => {
      expect(() => splitUint64(-1n)).toThrow('out of range for uint64');
    });

    it('should throw error for values exceeding uint64 range', () => {
      const maxUint64 = (1n << 64n) - 1n;
      expect(() => splitUint64(maxUint64 + 1n)).toThrow('out of range for uint64');
    });
  });

  describe('hexToFieldElement', () => {
    it('should handle hex strings with 0x prefix', () => {
      expect(hexToFieldElement('0x00')).toBe('0000000000000000');
      // hexToFieldElement does NOT reverse bytes, it just pads
      expect(hexToFieldElement('0x1234')).toBe('1234000000000000');
    });

    it('should handle hex strings without 0x prefix', () => {
      expect(hexToFieldElement('00')).toBe('0000000000000000');
      expect(hexToFieldElement('1234')).toBe('1234000000000000');
    });

    it('should pad to 8-byte boundary', () => {
      expect(hexToFieldElement('0x01')).toBe('0100000000000000');
      expect(hexToFieldElement('0x0123')).toBe('0123000000000000');
      expect(hexToFieldElement('0x012345')).toBe('0123450000000000');
    });

    it('should handle odd-length hex strings', () => {
      expect(hexToFieldElement('0x1')).toBe('0100000000000000');
      expect(hexToFieldElement('0x123')).toBe('0123000000000000');
    });

    it('should handle multiple 8-byte chunks', () => {
      const result = hexToFieldElement('0x' + '12'.repeat(20));
      expect(result.length % 16).toBe(0);
      // 20 bytes = 40 hex chars, padded to 24 bytes = 48 hex chars
      expect(result.length).toBe(48);
    });
  });

  describe('concatenateFieldElements', () => {
    it('should concatenate multiple field elements', () => {
      const result = concatenateFieldElements([
        '0100000000000000',
        '0200000000000000',
        '0300000000000000',
      ]);
      expect(result).toBe('010000000000000002000000000000000300000000000000');
    });

    it('should handle single element', () => {
      const result = concatenateFieldElements(['0100000000000000']);
      expect(result).toBe('0100000000000000');
    });

    it('should handle empty array', () => {
      const result = concatenateFieldElements([]);
      expect(result).toBe('');
    });

    it('should throw error for invalid length elements', () => {
      expect(() => concatenateFieldElements(['01'])).toThrow('invalid length');
      expect(() => concatenateFieldElements(['0100000000000000', '01'])).toThrow('invalid length');
    });

    it('should throw error for elements with odd lengths', () => {
      expect(() => concatenateFieldElements(['010000000000000'])).toThrow('invalid length');
    });

    it('should throw error for elements longer than 16 chars', () => {
      expect(() => concatenateFieldElements(['01000000000000000'])).toThrow('invalid length');
    });
  });

  describe('integration tests', () => {
    it('should handle conversion pipeline for uint32', () => {
      const value = 42;
      const fieldElement = uint32ToFieldElement(value);
      expect(fieldElement).toBe('2a00000000000000');
      expect(fieldElement.length).toBe(16);
    });

    it('should handle conversion pipeline for uint64 split', () => {
      const value = 0x123456789abcdefn;
      const [low, high] = splitUint64(value);
      const concatenated = concatenateFieldElements([low, high]);
      expect(concatenated).toBe('efcdab89000000006745230100000000');
    });

    it('should maintain consistency between uint64 and int64 for positive values', () => {
      const value = 1000n;
      const asUint = uint64ToFieldElement(value);
      const asInt = int64ToFieldElement(value);
      expect(asUint).toBe(asInt);
    });
  });
});
