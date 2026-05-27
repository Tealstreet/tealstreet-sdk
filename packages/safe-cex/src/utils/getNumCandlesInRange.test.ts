import { describe, it, expect } from 'vitest';
import { getNumCandlesInRange } from './getNumCandlesInRange';

describe('getNumCandlesInRange', () => {
  describe('basic candle counting', () => {
    it('should return 1 when from equals to', () => {
      const time = 1705329342; // Some arbitrary time
      expect(getNumCandlesInRange('1m', time, time)).toBe(1);
    });

    it('should count candles in a 1-minute range', () => {
      // 14:35:00 to 14:39:59 = 5 candles (35, 36, 37, 38, 39)
      const from = 1705329300; // 14:35:00
      const to = 1705329599;   // 14:39:59
      expect(getNumCandlesInRange('1m', from, to)).toBe(5);
    });

    it('should count candles in exact 5-minute range', () => {
      // 14:35:00 to 14:55:00 = 5 candles at 5m timeframe (35, 40, 45, 50, 55)
      const from = 1705329300; // 14:35:00
      const to = 1705330500;   // 14:55:00
      expect(getNumCandlesInRange('5m', from, to)).toBe(5);
    });

    it('should count candles for 1h timeframe', () => {
      // 12:00:00 to 16:00:00 = 5 candles (12, 13, 14, 15, 16)
      const from = 1705320000; // 12:00:00
      const to = 1705334400;   // 16:00:00
      expect(getNumCandlesInRange('1h', from, to)).toBe(5);
    });

    it('should count candles for 4h timeframe', () => {
      // 12:00:00 to 20:00:00 = 3 candles at 4h (12, 16, 20)
      const from = 1705320000; // 12:00:00
      const to = 1705348800;   // 20:00:00
      expect(getNumCandlesInRange('4h', from, to)).toBe(3);
    });
  });

  describe('non-boundary times', () => {
    it('should handle from time not at candle boundary', () => {
      // 14:35:42 to 14:39:00 in 1m = both in same 5-candle range
      const from = 1705329342; // 14:35:42
      const to = 1705329540;   // 14:39:00
      expect(getNumCandlesInRange('1m', from, to)).toBe(5);
    });

    it('should handle to time not at candle boundary', () => {
      // 14:35:00 to 14:37:30 in 1m = 3 candles (35, 36, 37)
      const from = 1705329300; // 14:35:00
      const to = 1705329450;   // 14:37:30
      expect(getNumCandlesInRange('1m', from, to)).toBe(3);
    });

    it('should handle both times not at candle boundary', () => {
      // 14:35:30 to 14:37:30 in 1m = 3 candles (35, 36, 37)
      const from = 1705329330; // 14:35:30
      const to = 1705329450;   // 14:37:30
      expect(getNumCandlesInRange('1m', from, to)).toBe(3);
    });
  });

  describe('edge cases', () => {
    it('should return 0 when from > to', () => {
      const from = 1705329600;
      const to = 1705329300;
      expect(getNumCandlesInRange('1m', from, to)).toBe(0);
    });

    it('should return 1 when times are within same candle', () => {
      // Both times within 14:35:xx
      const from = 1705329310; // 14:35:10
      const to = 1705329350;   // 14:35:50
      expect(getNumCandlesInRange('1m', from, to)).toBe(1);
    });

    it('should throw for unsupported timeframe', () => {
      expect(() =>
        getNumCandlesInRange('invalid' as any, 1705329300, 1705329600)
      ).toThrow('Unsupported timeframe: invalid');
    });
  });

  describe('daily and weekly timeframes', () => {
    it('should count candles for 1d timeframe', () => {
      // 2024-01-15 to 2024-01-18 = 4 days
      const from = 1705276800; // 2024-01-15 00:00:00
      const to = 1705536000;   // 2024-01-18 00:00:00
      expect(getNumCandlesInRange('1d', from, to)).toBe(4);
    });

    it('should count candles for 1w timeframe', () => {
      // Two weeks apart
      const from = 1705276800; // 2024-01-15
      const to = 1706486400;   // 2024-01-29 (two weeks later)
      expect(getNumCandlesInRange('1w', from, to)).toBe(3);
    });
  });

  describe('milliseconds handling', () => {
    it('should work with millisecond timestamps', () => {
      const fromMs = 1705329300000; // 14:35:00 in ms
      const toMs = 1705329599000;   // 14:39:59 in ms
      expect(getNumCandlesInRange('1m', fromMs, toMs)).toBe(5);
    });

    it('should give same result for ms and seconds', () => {
      const fromS = 1705329300;
      const toS = 1705329599;
      const fromMs = fromS * 1000;
      const toMs = toS * 1000;

      expect(getNumCandlesInRange('1m', fromS, toS)).toBe(
        getNumCandlesInRange('1m', fromMs, toMs)
      );
    });
  });
});
