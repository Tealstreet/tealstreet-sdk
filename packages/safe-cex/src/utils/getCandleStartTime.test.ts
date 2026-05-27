import { describe, it, expect } from 'vitest';
import { getCandleStartTime } from './getCandleStartTime';

describe('getCandleStartTime', () => {
  // Test timestamp: 2024-01-15 14:35:42 UTC
  // Seconds: 1705329342
  // Milliseconds: 1705329342000
  const testTimeSeconds = 1705329342;
  const testTimeMs = 1705329342000;

  describe('1m timeframe', () => {
    it('should return start of current minute (seconds input)', () => {
      // 14:35:42 should return 14:35:00
      const result = getCandleStartTime('1m', testTimeSeconds);
      expect(result).toBe(1705329300); // 14:35:00
    });

    it('should return start of current minute (ms input)', () => {
      const result = getCandleStartTime('1m', testTimeMs);
      expect(result).toBe(1705329300000); // 14:35:00 in ms
    });

    it('should return same time if already at minute boundary', () => {
      const boundaryTime = 1705329300; // 14:35:00
      expect(getCandleStartTime('1m', boundaryTime)).toBe(1705329300);
    });
  });

  describe('5m timeframe', () => {
    it('should return start of 5-minute period', () => {
      // 14:35:42 should return 14:35:00 (35 is a 5-min boundary)
      const result = getCandleStartTime('5m', testTimeSeconds);
      expect(result).toBe(1705329300); // 14:35:00
    });

    it('should handle time in middle of 5-min period', () => {
      // 14:37:00 should return 14:35:00
      const time = 1705329420; // 14:37:00
      const result = getCandleStartTime('5m', time);
      expect(result).toBe(1705329300); // 14:35:00
    });
  });

  describe('15m timeframe', () => {
    it('should return start of 15-minute period', () => {
      // 14:35:42 should return 14:30:00
      // 1705329342 / 900 = 1894810.38 -> floor -> 1894810 * 900 = 1705329000
      const result = getCandleStartTime('15m', testTimeSeconds);
      expect(result).toBe(1705329000); // 14:30:00
    });
  });

  describe('30m timeframe', () => {
    it('should return start of 30-minute period', () => {
      // 14:35:42 should return 14:30:00
      // 1705329342 / 1800 = 947405.19 -> floor -> 947405 * 1800 = 1705329000
      const result = getCandleStartTime('30m', testTimeSeconds);
      expect(result).toBe(1705329000); // 14:30:00
    });
  });

  describe('1h timeframe', () => {
    it('should return start of hour', () => {
      // 14:35:42 should return 14:00:00
      const result = getCandleStartTime('1h', testTimeSeconds);
      expect(result).toBe(1705327200); // 14:00:00
    });

    it('should handle ms input', () => {
      const result = getCandleStartTime('1h', testTimeMs);
      expect(result).toBe(1705327200000);
    });
  });

  describe('4h timeframe', () => {
    it('should return start of 4-hour period', () => {
      // 14:35:42 should return 12:00:00 (4h periods: 0, 4, 8, 12, 16, 20)
      const result = getCandleStartTime('4h', testTimeSeconds);
      expect(result).toBe(1705320000); // 12:00:00
    });
  });

  describe('1d timeframe', () => {
    it('should return start of day (UTC)', () => {
      // 14:35:42 should return 00:00:00 UTC
      const result = getCandleStartTime('1d', testTimeSeconds);
      expect(result).toBe(1705276800); // 2024-01-15 00:00:00 UTC
    });
  });

  describe('1w timeframe', () => {
    it('should return start of week', () => {
      // 2024-01-15 (Monday) 14:35:42
      // 1705329342 / 604800 = 2820.19 -> floor -> 2820 * 604800 = 1704931200
      // 1704931200 = 2024-01-11 00:00:00 (Thursday, week boundary from Unix epoch)
      const result = getCandleStartTime('1w', testTimeSeconds);
      expect(result).toBe(1704931200);
    });
  });

  describe('seconds timeframes', () => {
    it('should handle 1s timeframe', () => {
      const result = getCandleStartTime('1s', testTimeSeconds);
      expect(result).toBe(testTimeSeconds);
    });

    it('should handle 5s timeframe', () => {
      const result = getCandleStartTime('5s', testTimeSeconds);
      // 1705329342 / 5 = 341065868.4 -> floor -> 341065868 * 5 = 1705329340
      expect(result).toBe(1705329340);
    });

    it('should handle 15s timeframe', () => {
      const result = getCandleStartTime('15s', testTimeSeconds);
      // 1705329342 / 15 = 113688622.8 -> floor -> 113688622 * 15 = 1705329330
      expect(result).toBe(1705329330);
    });

    it('should handle 30s timeframe', () => {
      const result = getCandleStartTime('30s', testTimeSeconds);
      // 1705329342 / 30 = 56844311.4 -> floor -> 56844311 * 30 = 1705329330
      expect(result).toBe(1705329330);
    });
  });

  describe('millisecond detection', () => {
    it('should detect milliseconds when time > 1_000_000_000_000', () => {
      const timeMs = 1705329342123; // With some extra milliseconds
      const result = getCandleStartTime('1m', timeMs);
      expect(result).toBe(1705329300000); // Returns ms
    });

    it('should not convert seconds to milliseconds', () => {
      const timeSeconds = 1705329342;
      const result = getCandleStartTime('1m', timeSeconds);
      expect(result).toBe(1705329300); // Returns seconds
    });

    it('should handle edge case at boundary', () => {
      // Just under the threshold
      const timeJustUnder = 999_999_999_999;
      const result = getCandleStartTime('1h', timeJustUnder);
      expect(result).toBeLessThan(1_000_000_000_000);
    });
  });

  describe('edge cases', () => {
    it('should handle zero timestamp', () => {
      expect(getCandleStartTime('1m', 0)).toBe(0);
      expect(getCandleStartTime('1h', 0)).toBe(0);
      expect(getCandleStartTime('1d', 0)).toBe(0);
    });

    it('should handle very old timestamp', () => {
      // 1970-01-01 00:05:30
      const oldTime = 330;
      expect(getCandleStartTime('1m', oldTime)).toBe(300);
      expect(getCandleStartTime('1h', oldTime)).toBe(0);
    });
  });
});
