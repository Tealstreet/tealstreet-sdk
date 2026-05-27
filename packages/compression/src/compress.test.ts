import { describe, expect, it } from 'vitest';

import { compress } from './compress';

describe('compress', () => {
  describe('basic compression', () => {
    it('should compress a simple string', async () => {
      const input = 'Hello, World!';
      const result = await compress(input);
      expect(result).toBeInstanceOf(ArrayBuffer);
      expect(result.byteLength).toBeGreaterThan(0);
    });

    it('should return smaller output for repetitive data', async () => {
      const repetitiveData = 'a'.repeat(1000);
      const result = await compress(repetitiveData);
      // Compressed should be smaller than original for repetitive data
      expect(result.byteLength).toBeLessThan(repetitiveData.length);
    });

    it('should compress empty string', async () => {
      const result = await compress('');
      expect(result).toBeInstanceOf(ArrayBuffer);
    });

    it('should compress unicode characters', async () => {
      const input = '日本語テスト 🎉';
      const result = await compress(input);
      expect(result).toBeInstanceOf(ArrayBuffer);
      expect(result.byteLength).toBeGreaterThan(0);
    });

    it('should compress JSON data', async () => {
      const input = JSON.stringify({ foo: 'bar', nested: { value: 123 } });
      const result = await compress(input);
      expect(result).toBeInstanceOf(ArrayBuffer);
    });
  });

  describe('compression formats', () => {
    it('should use gzip by default', async () => {
      const input = 'test data';
      const result = await compress(input);
      // Gzip has specific header bytes
      const bytes = new Uint8Array(result);
      // Gzip magic numbers: 0x1f 0x8b
      expect(bytes[0]).toBe(0x1f);
      expect(bytes[1]).toBe(0x8b);
    });

    it('should support deflate format', async () => {
      const input = 'test data';
      const result = await compress(input, 'deflate');
      expect(result).toBeInstanceOf(ArrayBuffer);
      expect(result.byteLength).toBeGreaterThan(0);
    });

    it('should support deflate-raw format', async () => {
      const input = 'test data';
      const result = await compress(input, 'deflate-raw');
      expect(result).toBeInstanceOf(ArrayBuffer);
      expect(result.byteLength).toBeGreaterThan(0);
    });
  });

  describe('compression consistency', () => {
    it('should produce consistent output for same input', async () => {
      const input = 'consistent data';
      const result1 = await compress(input);
      const result2 = await compress(input);
      // Same input should produce same output
      expect(new Uint8Array(result1)).toEqual(new Uint8Array(result2));
    });

    it('should produce different output for different inputs', async () => {
      const result1 = await compress('data1');
      const result2 = await compress('data2');
      // Different inputs should produce different outputs
      const arr1 = new Uint8Array(result1);
      const arr2 = new Uint8Array(result2);
      expect(arr1).not.toEqual(arr2);
    });
  });
});
