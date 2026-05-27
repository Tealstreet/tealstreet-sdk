import { describe, expect, it } from 'vitest';
import { compress } from './compression';

/**
 * Note: The decompress function uses DecompressionStream which behaves differently
 * in Node.js vs browsers. The Node.js implementation expects Uint8Array/Buffer
 * instead of ArrayBuffer. Since this is a browser-targeted utility, we only test
 * the compress function in the Node.js test environment.
 *
 * Full round-trip tests would need to run in a browser environment.
 */
describe('compress', () => {
  describe('basic compression', () => {
    it('returns an ArrayBuffer', async () => {
      const result = await compress('test');

      expect(result).toBeInstanceOf(ArrayBuffer);
    });

    it('compresses a simple string', async () => {
      const result = await compress('Hello, World!');

      expect(result).toBeInstanceOf(ArrayBuffer);
      expect(result.byteLength).toBeGreaterThan(0);
    });

    it('compresses an empty string', async () => {
      const result = await compress('');

      expect(result).toBeInstanceOf(ArrayBuffer);
    });

    it('produces smaller output for compressible data', async () => {
      const original = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
      const originalSize = new TextEncoder().encode(original).length;

      const compressed = await compress(original);

      expect(compressed.byteLength).toBeLessThan(originalSize);
    });

    it('compresses JSON data', async () => {
      const data = {
        name: 'Test',
        values: [1, 2, 3, 4, 5],
        nested: { key: 'value' },
      };
      const original = JSON.stringify(data);

      const compressed = await compress(original);

      expect(compressed).toBeInstanceOf(ArrayBuffer);
      expect(compressed.byteLength).toBeGreaterThan(0);
    });
  });

  describe('compression efficiency', () => {
    it('compresses repeated patterns efficiently', async () => {
      const original = 'ABCD'.repeat(1000);

      const compressed = await compress(original);

      expect(compressed.byteLength).toBeLessThan(original.length);
    });

    it('handles large strings', async () => {
      const original = 'x'.repeat(10000);

      const compressed = await compress(original);

      expect(compressed).toBeInstanceOf(ArrayBuffer);
      expect(compressed.byteLength).toBeLessThan(original.length);
    });
  });

  describe('encoding options', () => {
    it('accepts gzip encoding (default)', async () => {
      const result = await compress('test', 'gzip');

      expect(result).toBeInstanceOf(ArrayBuffer);
    });

    it('accepts deflate encoding', async () => {
      const result = await compress('test', 'deflate');

      expect(result).toBeInstanceOf(ArrayBuffer);
    });

    it('accepts deflate-raw encoding', async () => {
      const result = await compress('test', 'deflate-raw');

      expect(result).toBeInstanceOf(ArrayBuffer);
    });
  });

  describe('edge cases', () => {
    it('handles string with special characters', async () => {
      const original = 'Line1\nLine2\tTab\rCarriage';

      const compressed = await compress(original);

      expect(compressed).toBeInstanceOf(ArrayBuffer);
      expect(compressed.byteLength).toBeGreaterThan(0);
    });

    it('handles unicode characters', async () => {
      const original = 'Hello, World!';

      const compressed = await compress(original);

      expect(compressed).toBeInstanceOf(ArrayBuffer);
      expect(compressed.byteLength).toBeGreaterThan(0);
    });
  });
});
