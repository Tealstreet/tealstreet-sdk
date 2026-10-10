import { describe, expect, it } from 'vitest';

import {
  calendarPeriodForChartSeconds,
  calendarPeriodForInterval,
  calendarPeriodStartMs,
  chartCandleCloseTime,
  chartCandleStartTime,
  createLiveCandleStart,
  getNumChartCandlesInRange,
  nextCalendarPeriodStartMs,
  nextChartCandleStartTime,
} from './calendarCandles';
import { getCandleStartTime } from './getCandleStartTime';
import { getNumCandlesInRange } from './getNumCandlesInRange';

const utc = (year: number, month: number, day: number, hour = 0, minute = 0) =>
  Date.UTC(year, month - 1, day, hour, minute);

describe('calendarPeriodStartMs', () => {
  it.each([
    // [year, month, length]: 28, 29 (leap), 30 and 31 days, and a year rollover.
    [2025, 2, 28],
    [2024, 2, 29],
    [2026, 4, 30],
    [2026, 1, 31],
    [2025, 12, 31],
  ])('%d-%d: the last minute is still that month', (year, month, length) => {
    const first = utc(year, month, 1);
    expect(calendarPeriodStartMs('1M', first)).toBe(first);
    expect(calendarPeriodStartMs('1M', utc(year, month, length, 23, 59) + 59_999)).toBe(first);
    const next = month === 12 ? utc(year + 1, 1, 1) : utc(year, month + 1, 1);
    expect(calendarPeriodStartMs('1M', next)).toBe(next);
    expect((next - first) / 86_400_000).toBe(length);
  });

  it('starts weeks on Monday, not the epoch floor Thursday', () => {
    const monday = utc(2026, 10, 5);
    for (const ts of [monday, utc(2026, 10, 8), utc(2026, 10, 11, 23, 59), utc(2026, 10, 12) - 1]) {
      expect(calendarPeriodStartMs('1W', ts)).toBe(monday);
    }
    expect(calendarPeriodStartMs('1W', utc(2026, 10, 12))).toBe(utc(2026, 10, 12));
    // Across a year boundary.
    expect(calendarPeriodStartMs('1W', utc(2026, 1, 1, 12))).toBe(utc(2025, 12, 29));
    expect(getCandleStartTime('1w', utc(2026, 10, 10))).toBe(utc(2026, 10, 8));
  });
});

describe('chartCandleStartTime', () => {
  it('keeps the units it is given', () => {
    expect(chartCandleStartTime('1M', utc(2026, 10, 31, 23, 59))).toBe(utc(2026, 10, 1));
    expect(chartCandleStartTime('1M', utc(2026, 10, 31, 23, 59) / 1000)).toBe(utc(2026, 10, 1) / 1000);
    expect(chartCandleStartTime('1w', utc(2026, 10, 11, 23, 59) / 1000)).toBe(utc(2026, 10, 5) / 1000);
  });

  it('is getCandleStartTime for every other interval', () => {
    const ts = utc(2026, 10, 10, 13, 37) / 1000;
    for (const interval of ['1m', '15m', '1h', '4h', '1d'] as const) {
      expect(chartCandleStartTime(interval, ts)).toBe(getCandleStartTime(interval, ts));
    }
  });
});

describe('calendar period lookups', () => {
  it('maps chart intervals and candle sizes', () => {
    expect(calendarPeriodForInterval('1w')).toBe('1W');
    expect(calendarPeriodForInterval('1M')).toBe('1M');
    expect(calendarPeriodForInterval('1m')).toBeUndefined();
    expect(calendarPeriodForInterval('1d')).toBeUndefined();
    expect(calendarPeriodForChartSeconds(604_800)).toBe('1W');
    expect(calendarPeriodForChartSeconds(2_592_000)).toBe('1M');
    expect(calendarPeriodForChartSeconds(86_400)).toBeUndefined();
  });
});

describe('next calendar candle', () => {
  it.each([
    [2025, 2, 28],
    [2024, 2, 29],
    [2026, 4, 30],
    [2026, 1, 31],
    [2025, 12, 31],
  ])('%d-%d rolls to the 1st of the next month, %d days later', (year, month, length) => {
    const first = utc(year, month, 1);
    const next = month === 12 ? utc(year + 1, 1, 1) : utc(year, month + 1, 1);
    expect(nextCalendarPeriodStartMs('1M', first)).toBe(next);
    expect(nextCalendarPeriodStartMs('1M', utc(year, month, length, 23, 59))).toBe(next);
    expect(nextChartCandleStartTime('1M', first / 1000)).toBe(next / 1000);
    expect(nextChartCandleStartTime('1M', first)).toBe(next);
  });

  it('steps Monday weeks across a year boundary', () => {
    expect(nextCalendarPeriodStartMs('1W', utc(2025, 12, 31))).toBe(utc(2026, 1, 5));
    expect(nextChartCandleStartTime('1w', utc(2025, 12, 29) / 1000)).toBe(utc(2026, 1, 5) / 1000);
  });

  it('is one interval past the epoch floor for every other interval', () => {
    const ts = utc(2026, 10, 10, 13, 37) / 1000;
    expect(nextChartCandleStartTime('1h', ts)).toBe(utc(2026, 10, 10, 14) / 1000);
    expect(nextChartCandleStartTime('1d', ts * 1000)).toBe(utc(2026, 10, 11));
  });
});

describe('chartCandleCloseTime', () => {
  it('closes a month on the 1st of the next, whatever its length', () => {
    expect(chartCandleCloseTime('1M', utc(2024, 2, 1))).toBe(utc(2024, 3, 1));
    expect(chartCandleCloseTime('1M', utc(2025, 2, 1) / 1000)).toBe(utc(2025, 3, 1) / 1000);
    expect(chartCandleCloseTime('1M', utc(2025, 12, 1))).toBe(utc(2026, 1, 1));
  });

  it('is relative to the candle, so an off-grid venue candle is still forming until it has lasted its length', () => {
    // OKX `1W` opens Sunday 16:00 UTC (Monday in Hong Kong).
    expect(chartCandleCloseTime('1w', utc(2026, 10, 4, 16))).toBe(utc(2026, 10, 11, 16));
    // OKX `1D` opens 16:00 UTC.
    expect(chartCandleCloseTime('1d', utc(2026, 10, 9, 16) / 1000)).toBe(utc(2026, 10, 10, 16) / 1000);
  });
});

describe('getNumChartCandlesInRange', () => {
  it('counts real months where a 30-day division undercounts', () => {
    // January to March is 59 days, under two nominal 30-day months.
    expect(getNumChartCandlesInRange('1M', utc(2025, 1, 1) / 1000, utc(2025, 3, 1) / 1000)).toBe(3);
    expect(getNumChartCandlesInRange('1M', utc(2025, 11, 15), utc(2026, 2, 3))).toBe(4);
    expect(getNumChartCandlesInRange('1M', utc(2026, 10, 1), utc(2026, 10, 1))).toBe(1);
    expect(getNumChartCandlesInRange('1M', utc(2026, 10, 1), utc(2026, 9, 1))).toBe(0);
  });

  it('counts Monday weeks', () => {
    // Monday 2025-12-29 .. Sunday 2026-01-11 is two weeks.
    expect(getNumChartCandlesInRange('1w', utc(2025, 12, 29), utc(2026, 1, 11, 23))).toBe(2);
  });

  it('is getNumCandlesInRange for every other interval', () => {
    const from = utc(2026, 10, 1) / 1000;
    const to = utc(2026, 10, 10, 13) / 1000;
    expect(getNumChartCandlesInRange('1h', from, to)).toBe(getNumCandlesInRange('1h', from, to));
  });
});

describe('createLiveCandleStart', () => {
  it('buckets trades into Monday weeks, not epoch-floor Thursdays', () => {
    const start = createLiveCandleStart('1w');
    expect(start(utc(2026, 10, 10, 12) / 1000)).toBe(utc(2026, 10, 5) / 1000);
    expect(start(utc(2026, 10, 11, 23, 59) / 1000)).toBe(utc(2026, 10, 5) / 1000);
    expect(start(utc(2026, 10, 12) / 1000)).toBe(utc(2026, 10, 12) / 1000);
  });

  it('buckets trades into calendar months across the rollover', () => {
    const start = createLiveCandleStart('1M');
    expect(start(utc(2026, 2, 28, 23, 59) / 1000)).toBe(utc(2026, 2, 1) / 1000);
    expect(start(utc(2026, 3, 1) / 1000)).toBe(utc(2026, 3, 1) / 1000);
    expect(start(utc(2026, 12, 31, 23, 59) / 1000)).toBe(utc(2026, 12, 1) / 1000);
    expect(start(utc(2027, 1, 1) / 1000)).toBe(utc(2027, 1, 1) / 1000);
    // Out-of-order input recomputes rather than reusing the cached bucket.
    expect(start(utc(2026, 5, 20) / 1000)).toBe(utc(2026, 5, 1) / 1000);
  });

  it('is the epoch floor for intraday intervals', () => {
    const start = createLiveCandleStart('15m');
    expect(start(utc(2026, 10, 10, 13, 37) / 1000)).toBe(utc(2026, 10, 10, 13, 30) / 1000);
  });
});
