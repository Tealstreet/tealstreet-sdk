import { describe, it, expect } from 'vitest';
import { parsePrecision } from './parsePrecision';

describe('parsePrecision', () => {
  it('should return undefined for undefined input', () => {
    expect(parsePrecision(undefined)).toBeUndefined();
  });

  it('should return undefined for non-numeric string', () => {
    expect(parsePrecision('abc')).toBeUndefined();
  });

  it('should return undefined for negative precision', () => {
    expect(parsePrecision('-1')).toBeUndefined();
  });

  it('should return "1" for precision 0', () => {
    expect(parsePrecision('0')).toBe('1');
  });

  it('should return "0.1" for precision 1', () => {
    expect(parsePrecision('1')).toBe('0.1');
  });

  it('should return "0.01" for precision 2', () => {
    expect(parsePrecision('2')).toBe('0.01');
  });

  it('should return "0.001" for precision 3', () => {
    expect(parsePrecision('3')).toBe('0.001');
  });

  it('should handle large precision values', () => {
    expect(parsePrecision('8')).toBe('0.00000001');
  });

  it('should return undefined for empty string', () => {
    expect(parsePrecision('')).toBeUndefined();
  });

  it('should handle precision string with leading zeros', () => {
    expect(parsePrecision('02')).toBe('0.01');
  });
});
