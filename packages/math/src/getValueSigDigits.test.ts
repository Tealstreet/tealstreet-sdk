import { describe, expect, it } from 'vitest';
import { getValueSigDigits } from './getValueSigDigits';

describe('getValueSigDigits', () => {
  it('should return 1 for zero', () => {
    expect(getValueSigDigits(0)).toBe(1);
    expect(getValueSigDigits('0')).toBe(1);
    expect(getValueSigDigits(-0)).toBe(1);
    expect(getValueSigDigits('0.0')).toBe(1);
  });

  it('should return 2 for values >= 1000', () => {
    expect(getValueSigDigits(1000)).toBe(2);
    expect(getValueSigDigits(1500)).toBe(2);
    expect(getValueSigDigits(9999)).toBe(2);
    expect(getValueSigDigits(1000000)).toBe(2);
    expect(getValueSigDigits('5000')).toBe(2);
    expect(getValueSigDigits(-1000)).toBe(2);
    expect(getValueSigDigits(-5000)).toBe(2);
  });

  it('should return 3 for values >= 10 and < 1000', () => {
    expect(getValueSigDigits(10)).toBe(3);
    expect(getValueSigDigits(50)).toBe(3);
    expect(getValueSigDigits(99.9)).toBe(3);
    expect(getValueSigDigits(999.9)).toBe(3);
    expect(getValueSigDigits('25.5')).toBe(3);
    expect(getValueSigDigits(-10)).toBe(3);
    expect(getValueSigDigits(-999)).toBe(3);
  });

  it('should return 4 for values >= 1 and < 10', () => {
    expect(getValueSigDigits(1)).toBe(4);
    expect(getValueSigDigits(1.5)).toBe(4);
    expect(getValueSigDigits(5)).toBe(4);
    expect(getValueSigDigits(9.99)).toBe(4);
    expect(getValueSigDigits('2.5')).toBe(4);
    expect(getValueSigDigits(-1)).toBe(4);
    expect(getValueSigDigits(-9.99)).toBe(4);
  });

  it('should return 5 for values >= 0.0001 and < 1', () => {
    expect(getValueSigDigits(0.0001)).toBe(5);
    expect(getValueSigDigits(0.001)).toBe(5);
    expect(getValueSigDigits(0.5)).toBe(5);
    expect(getValueSigDigits(0.999)).toBe(5);
    expect(getValueSigDigits('0.1')).toBe(5);
    expect(getValueSigDigits(-0.0001)).toBe(5);
    expect(getValueSigDigits(-0.999)).toBe(5);
  });

  it('should return 6 for values >= 0.00001 and < 0.0001', () => {
    expect(getValueSigDigits(0.00001)).toBe(6);
    expect(getValueSigDigits(0.00005)).toBe(6);
    expect(getValueSigDigits(0.00009)).toBe(6);
    expect(getValueSigDigits('0.00002')).toBe(6);
    expect(getValueSigDigits(-0.00001)).toBe(6);
    expect(getValueSigDigits(-0.00009)).toBe(6);
  });

  it('should return 7 for values >= 0.000001 and < 0.00001', () => {
    expect(getValueSigDigits(0.000001)).toBe(7);
    expect(getValueSigDigits(0.000005)).toBe(7);
    expect(getValueSigDigits(0.000009)).toBe(7);
    expect(getValueSigDigits('0.000002')).toBe(7);
    expect(getValueSigDigits(-0.000001)).toBe(7);
  });

  it('should return 8 for values >= 0.0000001 and < 0.000001', () => {
    expect(getValueSigDigits(0.0000001)).toBe(8);
    expect(getValueSigDigits(0.0000005)).toBe(8);
    expect(getValueSigDigits(0.0000009)).toBe(8);
    expect(getValueSigDigits('0.0000002')).toBe(8);
    expect(getValueSigDigits(-0.0000001)).toBe(8);
  });

  it('should return 9 for values >= 0.00000001 and < 0.0000001', () => {
    expect(getValueSigDigits(0.00000001)).toBe(9);
    expect(getValueSigDigits(0.00000005)).toBe(9);
    expect(getValueSigDigits(0.00000009)).toBe(9);
    expect(getValueSigDigits('0.00000002')).toBe(9);
    expect(getValueSigDigits(-0.00000001)).toBe(9);
  });

  it('should return 10 for values >= 0.000000001 and < 0.00000001', () => {
    expect(getValueSigDigits(0.000000001)).toBe(10);
    expect(getValueSigDigits(0.000000005)).toBe(10);
    expect(getValueSigDigits(0.000000009)).toBe(10);
    expect(getValueSigDigits('0.000000002')).toBe(10);
    expect(getValueSigDigits(-0.000000001)).toBe(10);
  });

  it('should return 11 for values >= 0.0000000001 and < 0.000000001', () => {
    expect(getValueSigDigits(0.0000000001)).toBe(11);
    expect(getValueSigDigits(0.0000000005)).toBe(11);
    expect(getValueSigDigits(0.0000000009)).toBe(11);
    expect(getValueSigDigits('0.0000000002')).toBe(11);
    expect(getValueSigDigits(-0.0000000001)).toBe(11);
  });

  it('should return 12 for values < 0.0000000001', () => {
    expect(getValueSigDigits(0.00000000001)).toBe(12);
    expect(getValueSigDigits(0.00000000005)).toBe(12);
    expect(getValueSigDigits(1e-12)).toBe(12);
    expect(getValueSigDigits('0.00000000002')).toBe(12);
    expect(getValueSigDigits(-0.00000000001)).toBe(12);
  });

  it('should handle string inputs', () => {
    expect(getValueSigDigits('1234')).toBe(2);
    expect(getValueSigDigits('12.34')).toBe(3);
    expect(getValueSigDigits('1.234')).toBe(4);
    expect(getValueSigDigits('0.1234')).toBe(5);
    expect(getValueSigDigits('0.00001234')).toBe(6);
  });

  it('should handle negative values using absolute value', () => {
    expect(getValueSigDigits(-1234)).toBe(2);
    expect(getValueSigDigits(-12.34)).toBe(3);
    expect(getValueSigDigits(-1.234)).toBe(4);
    expect(getValueSigDigits(-0.1234)).toBe(5);
    expect(getValueSigDigits('-0.00001234')).toBe(6);
  });

  it('should handle edge cases', () => {
    expect(getValueSigDigits('999.99')).toBe(3);
    expect(getValueSigDigits('1000.01')).toBe(2);
    expect(getValueSigDigits('9.99999')).toBe(4);
    expect(getValueSigDigits('10.00001')).toBe(3);
    expect(getValueSigDigits('0.99999')).toBe(5);
    expect(getValueSigDigits('1.00001')).toBe(4);
  });

  it('should handle invalid inputs that parseFloat can parse', () => {
    expect(getValueSigDigits('12.34abc')).toBe(3); // parseFloat('12.34abc') = 12.34
    expect(getValueSigDigits('')).toBe(12); // parseFloat('') = NaN, Math.abs(NaN) = NaN, all comparisons with NaN are false, falls to default (12)
    expect(getValueSigDigits('abc')).toBe(12); // parseFloat('abc') = NaN, falls to default (12)
  });
});