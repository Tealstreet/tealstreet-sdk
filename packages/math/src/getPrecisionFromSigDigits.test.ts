import { describe, expect, it } from 'vitest';
import { getPrecisionFromSigDigits } from './getPrecisionFromSigDigits';

describe('getPrecisionFromSigDigits', () => {
  it('should return 0.1 for 1 significant digit', () => {
    expect(getPrecisionFromSigDigits(1)).toBe(0.1);
  });

  it('should return 0.01 for 2 significant digits', () => {
    expect(getPrecisionFromSigDigits(2)).toBe(0.01);
  });

  it('should return 0.001 for 3 significant digits', () => {
    expect(getPrecisionFromSigDigits(3)).toBe(0.001);
  });

  it('should return 1 for 0 significant digits', () => {
    expect(getPrecisionFromSigDigits(0)).toBe(1);
  });

  it('should handle large numbers of significant digits', () => {
    expect(getPrecisionFromSigDigits(10)).toBe(0.0000000001);
  });

  it('should handle negative significant digits', () => {
    expect(getPrecisionFromSigDigits(-1)).toBe(10);
    expect(getPrecisionFromSigDigits(-2)).toBe(100);
  });
});