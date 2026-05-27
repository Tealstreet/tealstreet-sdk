import { describe, expect, it } from 'vitest';
import { stripCurrency } from './stripCurrency';

describe('stripCurrency', () => {
  describe('basic functionality', () => {
    it('removes trailing currency suffix from formatted value', () => {
      expect(stripCurrency('0.001 BTC')).toBe('0.001');
      expect(stripCurrency('1,234.56 USDT')).toBe('1,234.56');
      expect(stripCurrency('100 ETH')).toBe('100');
    });

    it('handles values with $ prefix and currency suffix', () => {
      expect(stripCurrency('+$0.09 USD')).toBe('+$0.09');
      expect(stripCurrency('$1,234.56 USDT')).toBe('$1,234.56');
      expect(stripCurrency('-$500.00 USD')).toBe('-$500.00');
    });

    it('returns original value when no space is present', () => {
      expect(stripCurrency('123')).toBe('123');
      expect(stripCurrency('$1,234.56')).toBe('$1,234.56');
      expect(stripCurrency('BTC')).toBe('BTC');
    });
  });

  describe('edge cases', () => {
    it('returns empty string for empty input', () => {
      expect(stripCurrency('')).toBe('');
    });

    it('handles null and undefined values gracefully', () => {
      expect(stripCurrency(null as unknown as string)).toBe(null);
      expect(stripCurrency(undefined as unknown as string)).toBe(undefined);
    });

    it('handles string with multiple spaces', () => {
      expect(stripCurrency('1,234.56 USD EUR')).toBe('1,234.56 USD');
      expect(stripCurrency('value with multiple spaces')).toBe('value with multiple');
    });

    it('handles string with only a space', () => {
      expect(stripCurrency(' ')).toBe('');
    });

    it('handles string starting with space', () => {
      expect(stripCurrency(' BTC')).toBe('');
    });
  });

  describe('negative values', () => {
    it('preserves negative sign when stripping currency', () => {
      expect(stripCurrency('-0.001 BTC')).toBe('-0.001');
      expect(stripCurrency('-1,234.56 USD')).toBe('-1,234.56');
    });
  });
});
