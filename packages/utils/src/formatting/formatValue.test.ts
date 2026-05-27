import { describe, expect, it, vi } from 'vitest';
import { formatValue } from './formatValue';

describe('formatValue', () => {
  describe('basic formatting with hideCurrency=true (default)', () => {
    it('formats numbers with $ prefix and thousands separator', () => {
      expect(formatValue(1234.56)).toBe('$1,234.56');
      expect(formatValue(1000000)).toBe('$1,000,000.00');
      expect(formatValue(0)).toBe('$0.00');
    });

    it('formats negative numbers correctly', () => {
      expect(formatValue(-1234.56)).toBe('-$1,234.56');
      expect(formatValue(-0.01)).toBe('-$0.01');
    });

    it('uses default 2 decimal places', () => {
      expect(formatValue(100)).toBe('$100.00');
      expect(formatValue(99.9)).toBe('$99.90');
    });

    it('respects custom minimumFractionDigits', () => {
      expect(formatValue(0.00012345, 'USD', true, 8)).toBe('$0.00012345');
      expect(formatValue(1234, 'USD', true, 0)).toBe('$1,234');
      expect(formatValue(1234.5678, 'USD', true, 4)).toBe('$1,234.5678');
    });
  });

  describe('formatting with hideCurrency=false', () => {
    it('formats with currency suffix instead of $ prefix', () => {
      expect(formatValue(1234.56, 'USD', false)).toBe('1,234.56 USD');
      expect(formatValue(1234.56, 'BTC', false)).toBe('1,234.56 BTC');
      expect(formatValue(1234.56, 'ETH', false)).toBe('1,234.56 ETH');
    });

    it('formats with custom precision and currency suffix', () => {
      expect(formatValue(0.12345678, 'BTC', false, 8)).toBe('0.12345678 BTC');
      expect(formatValue(1000, 'USDT', false, 0)).toBe('1,000 USDT');
    });
  });

  describe('masked/placeholder values', () => {
    it('returns masked values with currency suffix unchanged', () => {
      expect(formatValue('****', 'USD')).toBe('**** USD');
      expect(formatValue('*****', 'BTC')).toBe('***** BTC');
      expect(formatValue('*hidden*', 'ETH')).toBe('*hidden* ETH');
    });
  });

  describe('string input', () => {
    it('converts numeric strings to formatted values', () => {
      expect(formatValue('1234.56')).toBe('$1,234.56');
      expect(formatValue('0.001', 'BTC', false, 8)).toBe('0.00100000 BTC');
    });

    it('handles NaN from invalid string input', () => {
      expect(formatValue('invalid')).toBe('$NaN');
    });
  });

  describe('edge cases', () => {
    it('handles very small numbers', () => {
      expect(formatValue(0.00000001, 'BTC', true, 8)).toBe('$0.00000001');
    });

    it('handles very large numbers', () => {
      expect(formatValue(999999999999.99)).toBe('$999,999,999,999.99');
    });

    it('handles zero', () => {
      expect(formatValue(0)).toBe('$0.00');
      expect(formatValue(0, 'BTC', false, 8)).toBe('0.00000000 BTC');
    });
  });

  describe('formatter cache', () => {
    it('reuses Intl.NumberFormat instances for the same precision', async () => {
      vi.resetModules();
      const OriginalNumberFormat = Intl.NumberFormat;
      const numberFormatSpy = vi
        .spyOn(Intl, 'NumberFormat')
        .mockImplementation(
          function (
            ...args: ConstructorParameters<typeof Intl.NumberFormat>
          ): Intl.NumberFormat {
            return new OriginalNumberFormat(...args);
          } as typeof Intl.NumberFormat,
        );

      try {
        const { formatValue: isolatedFormatValue } = await import('./formatValue');

        expect(isolatedFormatValue(1, 'USD', true, 6)).toBe('$1.000000');
        expect(isolatedFormatValue(2, 'BTC', false, 6)).toBe('2.000000 BTC');
        expect(isolatedFormatValue(3, 'USD', true, 7)).toBe('$3.0000000');

        expect(numberFormatSpy).toHaveBeenCalledTimes(2);
      } finally {
        numberFormatSpy.mockRestore();
      }
    });
  });
});
