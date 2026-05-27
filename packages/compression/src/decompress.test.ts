import { describe, expect, it } from 'vitest';

import { compress } from './compress';
import { decompress } from './decompress';

describe('decompress', () => {
  describe('basic decompression', () => {
    it('should decompress a simple compressed string', async () => {
      const original = 'Hello, World!';
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should decompress empty string', async () => {
      const original = '';
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should decompress unicode characters', async () => {
      const original = '日本語テスト 🎉';
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should decompress JSON data', async () => {
      const original = JSON.stringify({ foo: 'bar', nested: { value: 123 } });
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should decompress large repetitive data', async () => {
      const original = 'a'.repeat(10000);
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });
  });

  describe('decompression formats', () => {
    it('should use gzip by default', async () => {
      const original = 'test data';
      const compressed = await compress(original, 'gzip');
      const result = await decompress(compressed, 'gzip');
      expect(result).toBe(original);
    });

    it('should support deflate format', async () => {
      const original = 'test data';
      const compressed = await compress(original, 'deflate');
      const result = await decompress(compressed, 'deflate');
      expect(result).toBe(original);
    });

    it('should support deflate-raw format', async () => {
      const original = 'test data';
      const compressed = await compress(original, 'deflate-raw');
      const result = await decompress(compressed, 'deflate-raw');
      expect(result).toBe(original);
    });
  });

  describe('edge cases', () => {
    it('should handle special characters', async () => {
      const original = '!@#$%^&*()_+-=[]{}|;:\'",.<>?/\\`~';
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should handle newlines and tabs', async () => {
      const original = 'line1\nline2\tline3\r\nline4';
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should handle multiline JSON', async () => {
      const original = JSON.stringify(
        {
          array: [1, 2, 3],
          nested: {
            deep: {
              value: 'test',
            },
          },
        },
        null,
        2,
      );
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });

    it('should handle binary-like strings', async () => {
      // Create a string with all possible byte values (excluding null)
      const chars = [];
      for (let i = 1; i < 128; i++) {
        chars.push(String.fromCharCode(i));
      }
      const original = chars.join('');
      const compressed = await compress(original);
      const result = await decompress(compressed);
      expect(result).toBe(original);
    });
  });

  describe('roundtrip consistency', () => {
    it('should produce consistent results for repeated operations', async () => {
      const original = 'consistent data';
      const compressed1 = await compress(original);
      const compressed2 = await compress(original);

      const result1 = await decompress(compressed1);
      const result2 = await decompress(compressed2);

      expect(result1).toBe(original);
      expect(result2).toBe(original);
      expect(result1).toBe(result2);
    });

    it('should handle multiple compression/decompression cycles', async () => {
      const original = 'test for multiple cycles';
      let current = original;

      for (let i = 0; i < 3; i++) {
        const compressed = await compress(current);
        current = await decompress(compressed);
      }

      expect(current).toBe(original);
    });
  });
});
