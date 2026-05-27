import { describe, it, expect } from 'vitest';
import { normalizeEpochToSeconds } from './normalizeEpochToSeconds';

describe('normalizeEpochToSeconds', () => {
  it('should return seconds as-is', () => {
    const seconds = 1705329342;
    expect(normalizeEpochToSeconds(seconds)).toBe(seconds);
  });

  it('should convert milliseconds to seconds', () => {
    const ms = 1705329342000;
    expect(normalizeEpochToSeconds(ms)).toBe(1705329342);
  });

  it('should handle edge case at threshold', () => {
    // Just under threshold - should return as-is
    const justUnder = 999_999_999_999;
    expect(normalizeEpochToSeconds(justUnder)).toBe(justUnder);

    // Just over threshold - should divide
    const justOver = 1_000_000_000_001;
    expect(normalizeEpochToSeconds(justOver)).toBe(1000000000);
  });

  it('should handle zero', () => {
    expect(normalizeEpochToSeconds(0)).toBe(0);
  });

  it('should floor milliseconds (not round)', () => {
    const msWithFraction = 1705329342999;
    expect(normalizeEpochToSeconds(msWithFraction)).toBe(1705329342);
  });

  it('should handle old timestamps (1970s)', () => {
    const old = 86400; // 1 day after epoch
    expect(normalizeEpochToSeconds(old)).toBe(86400);
  });

  it('should handle future timestamps in seconds', () => {
    const futureSeconds = 2000000000; // ~2033
    expect(normalizeEpochToSeconds(futureSeconds)).toBe(futureSeconds);
  });

  it('should handle future timestamps in milliseconds', () => {
    const futureMs = 2000000000000; // ~2033 in ms
    expect(normalizeEpochToSeconds(futureMs)).toBe(2000000000);
  });
});
