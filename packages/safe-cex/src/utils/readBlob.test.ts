import { describe, it, expect, vi, beforeEach } from 'vitest';
import { deflate } from 'pako';

// Mock pako's inflate function
vi.mock('pako', () => ({
  inflate: vi.fn(),
}));

import { inflate } from 'pako';
import { readBlob } from './readBlob';

describe('readBlob', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('with valid compressed data', () => {
    it('should decompress and convert blob to string', async () => {
      const testString = 'Hello, World!';
      // Create a Uint8Array that represents the test string as UTF-16 characters
      const charCodes = testString.split('').map((c) => c.charCodeAt(0));
      const inflatedData = new Uint8Array(charCodes);

      // Mock inflate to return our test data
      vi.mocked(inflate).mockReturnValueOnce(inflatedData);

      // Create a mock blob
      const mockBlob = new Blob([new Uint8Array([1, 2, 3])]);

      const result = await readBlob(mockBlob);

      expect(result).toBe(testString);
      expect(inflate).toHaveBeenCalledTimes(1);
    });

    it('should pass the blob buffer to inflate', async () => {
      const testData = new Uint8Array([72, 101, 108, 108, 111]); // 'Hello'
      const inflatedData = new Uint8Array([72, 101, 108, 108, 111]);

      vi.mocked(inflate).mockReturnValueOnce(inflatedData);

      const mockBlob = new Blob([testData]);
      await readBlob(mockBlob);

      // inflate should be called with a Uint8Array
      expect(inflate).toHaveBeenCalledTimes(1);
      const callArg = vi.mocked(inflate).mock.calls[0][0];
      expect(callArg).toBeInstanceOf(Uint8Array);
    });

    it('should handle empty blob', async () => {
      vi.mocked(inflate).mockReturnValueOnce(new Uint8Array([]));

      const mockBlob = new Blob([]);
      const result = await readBlob(mockBlob);

      expect(result).toBe('');
    });

    it('should handle single character', async () => {
      const inflatedData = new Uint8Array([65]); // 'A'
      vi.mocked(inflate).mockReturnValueOnce(inflatedData);

      const mockBlob = new Blob([new Uint8Array([1])]);
      const result = await readBlob(mockBlob);

      expect(result).toBe('A');
    });

    it('should handle special characters', async () => {
      // Characters like é (233), ñ (241)
      const inflatedData = new Uint8Array([233, 241]);
      vi.mocked(inflate).mockReturnValueOnce(inflatedData);

      const mockBlob = new Blob([new Uint8Array([1, 2])]);
      const result = await readBlob(mockBlob);

      expect(result).toBe('éñ');
    });
  });

  describe('async behavior', () => {
    it('should return a promise', () => {
      vi.mocked(inflate).mockReturnValueOnce(new Uint8Array([]));
      const mockBlob = new Blob([]);

      const result = readBlob(mockBlob);

      expect(result).toBeInstanceOf(Promise);
    });

    it('should resolve with the decoded string', async () => {
      const inflatedData = new Uint8Array([84, 101, 115, 116]); // 'Test'
      vi.mocked(inflate).mockReturnValueOnce(inflatedData);

      const mockBlob = new Blob([new Uint8Array([1, 2, 3, 4])]);
      const result = await readBlob(mockBlob);

      expect(result).toBe('Test');
    });
  });

  describe('error handling', () => {
    it('should propagate errors from inflate', async () => {
      vi.mocked(inflate).mockImplementationOnce(() => {
        throw new Error('Invalid compressed data');
      });

      const mockBlob = new Blob([new Uint8Array([1, 2, 3])]);

      await expect(readBlob(mockBlob)).rejects.toThrow('Invalid compressed data');
    });
  });
});
