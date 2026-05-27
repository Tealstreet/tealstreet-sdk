import { describe, expect, it } from 'vitest';

import { ExchangeOptions } from '../types';
import { isNonPrivateDataAccount } from './isNonPrivateDataAccount';

describe('isNonPrivateDataAccount', () => {
  describe('publicDataOnly flag', () => {
    it('should return true when publicDataOnly is true', () => {
      const options: ExchangeOptions = { publicDataOnly: true, key: '', secret: '' };
      expect(isNonPrivateDataAccount('binance', options)).toBe(true);
    });

    it('should return false when publicDataOnly is false with credentials', () => {
      const options: ExchangeOptions = { publicDataOnly: false, key: 'key', secret: 'secret' };
      expect(isNonPrivateDataAccount('binance', options)).toBe(false);
    });
  });

  describe('empty credentials', () => {
    it('should return true when key is empty string', () => {
      const options: ExchangeOptions = { key: '', secret: 'secret' };
      expect(isNonPrivateDataAccount('binance', options)).toBe(true);
    });

    it('should return true when secret is empty string', () => {
      const options: ExchangeOptions = { key: 'key', secret: '' };
      expect(isNonPrivateDataAccount('binance', options)).toBe(true);
    });

    it('should return true when both key and secret are empty', () => {
      const options: ExchangeOptions = { key: '', secret: '' };
      expect(isNonPrivateDataAccount('binance', options)).toBe(true);
    });
  });

  describe('hyperliquid special case', () => {
    it('should return false for hyperliquid even with empty credentials', () => {
      const options: ExchangeOptions = { key: '', secret: '' };
      expect(isNonPrivateDataAccount('hyperliquid', options)).toBe(false);
    });

    it('should return false for hyperliquid with publicDataOnly', () => {
      const options: ExchangeOptions = { publicDataOnly: true, key: '', secret: '' };
      expect(isNonPrivateDataAccount('hyperliquid', options)).toBe(false);
    });

    it('should return false for hyperliquidspot even with empty credentials', () => {
      const options: ExchangeOptions = { key: '', secret: '' };
      expect(isNonPrivateDataAccount('hyperliquidspot', options)).toBe(false);
    });
  });

  describe('valid credentials', () => {
    it('should return false when key and secret are provided', () => {
      const options: ExchangeOptions = { key: 'api-key', secret: 'api-secret' };
      expect(isNonPrivateDataAccount('binance', options)).toBe(false);
    });

    it('should return false for various exchanges with credentials', () => {
      const options: ExchangeOptions = { key: 'key', secret: 'secret' };
      expect(isNonPrivateDataAccount('bybit', options)).toBe(false);
      expect(isNonPrivateDataAccount('okx', options)).toBe(false);
      expect(isNonPrivateDataAccount('bitmex', options)).toBe(false);
    });
  });

  describe('undefined parameters', () => {
    it('should return false when options is undefined', () => {
      expect(isNonPrivateDataAccount('binance', undefined)).toBe(false);
    });

    it('should return false when exchangeName is undefined', () => {
      const options: ExchangeOptions = { publicDataOnly: true, key: '', secret: '' };
      expect(isNonPrivateDataAccount(undefined, options)).toBe(true);
    });

    it('should return false when both are undefined', () => {
      expect(isNonPrivateDataAccount(undefined, undefined)).toBe(false);
    });
  });
});
