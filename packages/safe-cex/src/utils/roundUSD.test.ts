import { describe, it, expect } from 'vitest';
import { roundUSD } from './roundUSD';

describe('roundUSD', () => {
  it('should round to 2 decimal places', () => {
    expect(roundUSD(1.234)).toBe(1.23);
    expect(roundUSD(1.235)).toBe(1.24);
    expect(roundUSD(1.236)).toBe(1.24);
  });

  it('should handle whole numbers', () => {
    expect(roundUSD(100)).toBe(100);
  });

  it('should handle zero', () => {
    expect(roundUSD(0)).toBe(0);
  });

  it('should handle negative values', () => {
    expect(roundUSD(-1.234)).toBe(-1.23);
    expect(roundUSD(-1.235)).toBe(-1.24);
  });

  it('should handle values already at 2 decimals', () => {
    expect(roundUSD(1.25)).toBe(1.25);
  });

  it('should handle large values', () => {
    expect(roundUSD(123456.789)).toBe(123456.79);
  });

  it('should handle very small values', () => {
    expect(roundUSD(0.001)).toBe(0);
    expect(roundUSD(0.005)).toBe(0.01);
  });
});
