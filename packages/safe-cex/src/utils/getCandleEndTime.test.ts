import { describe, it, expect } from 'vitest';
import { getCandleEndTime } from './getCandleEndTime';

describe('getCandleEndTime', () => {
  // Test timestamp: 2024-01-15 14:35:42 UTC
  // Seconds: 1705329342
  // Milliseconds: 1705329342000
  const testTimeSeconds = 1705329342;
  const testTimeMs = 1705329342000;

  describe('1m timeframe', () => {
    it('should return end of current minute (seconds input)', () => {
      // 14:35:42 should return 14:36:00
      const result = getCandleEndTime('1m', testTimeSeconds);
      expect(result).toBe(1705329360); // 14:36:00
    });

    it('should return end of current minute (ms input)', () => {
      const result = getCandleEndTime('1m', testTimeMs);
      expect(result).toBe(1705329360000); // 14:36:00 in ms
    });

    it('should return end of next candle if at minute boundary', () => {
      const boundaryTime = 1705329300; // 14:35:00
      // At boundary, should return end of THIS candle = 14:36:00
      expect(getCandleEndTime('1m', boundaryTime)).toBe(1705329360);
    });
  });

  describe('5m timeframe', () => {
    it('should return end of 5-minute period', () => {
      // 14:35:42 should return 14:40:00
      const result = getCandleEndTime('5m', testTimeSeconds);
      expect(result).toBe(1705329600); // 14:40:00
    });

    it('should handle time at 5-min boundary', () => {
      // 14:35:00 is a boundary, should return 14:40:00
      const time = 1705329300; // 14:35:00
      const result = getCandleEndTime('5m', time);
      expect(result).toBe(1705329600); // 14:40:00
    });
  });

  describe('15m timeframe', () => {
    it('should return end of 15-minute period', () => {
      // 14:35:42 should return 14:45:00
      // 1705329342 / 900 = 1894810.38 -> ceil -> 1894811 * 900 = 1705329900
      const result = getCandleEndTime('15m', testTimeSeconds);
      expect(result).toBe(1705329900); // 14:45:00
    });
  });

  describe('30m timeframe', () => {
    it('should return end of 30-minute period', () => {
      // 14:35:42 should return 15:00:00
      // 1705329342 / 1800 = 947405.19 -> ceil -> 947406 * 1800 = 1705330800
      const result = getCandleEndTime('30m', testTimeSeconds);
      expect(result).toBe(1705330800); // 15:00:00
    });
  });

  describe('1h timeframe', () => {
    it('should return end of hour', () => {
      // 14:35:42 should return 15:00:00
      const result = getCandleEndTime('1h', testTimeSeconds);
      expect(result).toBe(1705330800); // 15:00:00
    });

    it('should handle ms input', () => {
      const result = getCandleEndTime('1h', testTimeMs);
      expect(result).toBe(1705330800000);
    });

    it('should return end of next hour when at hour boundary', () => {
      const hourBoundary = 1705327200; // 14:00:00
      const result = getCandleEndTime('1h', hourBoundary);
      expect(result).toBe(1705330800); // 15:00:00
    });
  });

  describe('4h timeframe', () => {
    it('should return end of 4-hour period', () => {
      // 14:35:42 should return 16:00:00 (4h periods: 0, 4, 8, 12, 16, 20)
      const result = getCandleEndTime('4h', testTimeSeconds);
      expect(result).toBe(1705334400); // 16:00:00
    });
  });

  describe('1d timeframe', () => {
    it('should return end of day (start of next day UTC)', () => {
      // 14:35:42 should return next day 00:00:00 UTC
      const result = getCandleEndTime('1d', testTimeSeconds);
      expect(result).toBe(1705363200); // 2024-01-16 00:00:00 UTC
    });

    it('should return end of next day when at day boundary', () => {
      const dayBoundary = 1705276800; // 2024-01-15 00:00:00 UTC
      const result = getCandleEndTime('1d', dayBoundary);
      expect(result).toBe(1705363200); // 2024-01-16 00:00:00 UTC
    });
  });

  describe('1w timeframe', () => {
    it('should return end of week', () => {
      // 2024-01-15 14:35:42 should return end of week period
      // 1705329342 / 604800 = 2820.65... -> ceil -> 2821 * 604800 = 1705536000
      const result = getCandleEndTime('1w', testTimeSeconds);
      expect(result).toBe(1705536000);
    });
  });

  describe('seconds timeframes', () => {
    it('should handle 1s timeframe', () => {
      // 1s is a boundary, so should return next second
      const result = getCandleEndTime('1s', testTimeSeconds);
      expect(result).toBe(testTimeSeconds + 1);
    });

    it('should handle 5s timeframe', () => {
      const result = getCandleEndTime('5s', testTimeSeconds);
      // 1705329342 / 5 = 341065868.4 -> ceil -> 341065869 * 5 = 1705329345
      expect(result).toBe(1705329345);
    });

    it('should handle 15s timeframe', () => {
      const result = getCandleEndTime('15s', testTimeSeconds);
      // 1705329342 / 15 = 113688622.8 -> ceil -> 113688623 * 15 = 1705329345
      expect(result).toBe(1705329345);
    });

    it('should handle 30s timeframe', () => {
      const result = getCandleEndTime('30s', testTimeSeconds);
      // 1705329342 / 30 = 56844311.4 -> ceil -> 56844312 * 30 = 1705329360
      expect(result).toBe(1705329360);
    });
  });

  describe('millisecond detection', () => {
    it('should detect milliseconds when time > 1_000_000_000_000', () => {
      const timeMs = 1705329342123;
      const result = getCandleEndTime('1m', timeMs);
      expect(result).toBe(1705329360000);
    });

    it('should not convert seconds to milliseconds', () => {
      const result = getCandleEndTime('1m', testTimeSeconds);
      expect(result).toBe(1705329360);
    });
  });

  describe('edge cases', () => {
    it('should handle zero timestamp at boundary', () => {
      // 0 is a boundary, so should return 0 + timeframe
      expect(getCandleEndTime('1m', 0)).toBe(60);
      expect(getCandleEndTime('1h', 0)).toBe(3600);
      expect(getCandleEndTime('1d', 0)).toBe(86400);
    });
  });
});
