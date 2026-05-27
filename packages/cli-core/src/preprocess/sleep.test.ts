import { describe, it, expect } from 'vitest';
import { parseDuration, MAX_SLEEP_MS } from './sleep';

describe('parseDuration', () => {
  it('treats bare number as seconds', () => {
    expect(parseDuration('5')).toBe(5000);
  });

  it('parses ms', () => {
    expect(parseDuration('250ms')).toBe(250);
  });

  it('parses s', () => {
    expect(parseDuration('30s')).toBe(30_000);
  });

  it('parses m', () => {
    expect(parseDuration('2m')).toBe(120_000);
  });

  it('accepts decimals', () => {
    expect(parseDuration('1.5s')).toBe(1500);
  });

  it('accepts whitespace between number and unit', () => {
    expect(parseDuration('5 s')).toBe(5000);
  });

  it('is case insensitive', () => {
    expect(parseDuration('30S')).toBe(30_000);
    expect(parseDuration('2M')).toBe(120_000);
  });

  it('caps at 10 minutes', () => {
    expect(parseDuration('10m')).toBe(MAX_SLEEP_MS);
    expect(() => parseDuration('11m')).toThrow(/10 minute cap/);
    expect(() => parseDuration('601s')).toThrow(/10 minute cap/);
  });

  it('rejects empty input', () => {
    expect(() => parseDuration('')).toThrow(/missing duration/);
    expect(() => parseDuration('   ')).toThrow(/missing duration/);
  });

  it('rejects garbage', () => {
    expect(() => parseDuration('abc')).toThrow(/invalid duration/);
    expect(() => parseDuration('5x')).toThrow(/invalid duration/);
    expect(() => parseDuration('-5s')).toThrow(/invalid duration/);
  });
});
