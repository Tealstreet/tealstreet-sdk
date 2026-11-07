import { describe, expect, it } from 'vitest';

import {
  normalizeBybitExecutionTimestampMs,
  normalizeBybitExecutionTimestampSeconds,
} from './reference/bybit/bybit.execution-time';

describe('Bybit execution timestamp normalization', () => {
  it('accepts seconds, milliseconds, and announced microsecond precision', () => {
    expect(normalizeBybitExecutionTimestampMs('1793096306')).toBe(1_793_096_306_000);
    expect(normalizeBybitExecutionTimestampMs('1793096306496')).toBe(1_793_096_306_496);
    expect(normalizeBybitExecutionTimestampMs('1793096306496430')).toBe(1_793_096_306_496);

    expect(normalizeBybitExecutionTimestampSeconds('1793096306496430')).toBe(1_793_096_306);
  });

  it('rejects empty or invalid timestamp values', () => {
    expect(normalizeBybitExecutionTimestampMs('')).toBeUndefined();
    expect(normalizeBybitExecutionTimestampMs('not-a-time')).toBeUndefined();
    expect(normalizeBybitExecutionTimestampMs(0)).toBeUndefined();
  });
});
