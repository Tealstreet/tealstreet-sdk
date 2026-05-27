import { describe, it, expect } from 'vitest';
import {
  isUsdLikeCurrency,
  isBtcLikeCurrency,
  isEquivalentCurrency,
  isBtcLikeSymbol,
  getCurrencySigDigits,
  getCurrencyPrecision,
  BTC_SIG_DIGITS,
  USD_SIG_DIGITS,
  DEFAULT_SIG_DIGITS,
} from './currencyUtils';

describe('currencyUtils', () => {
  describe('isUsdLikeCurrency', () => {
    it('should return true for USD-like currencies', () => {
      expect(isUsdLikeCurrency('USD')).toBe(true);
      expect(isUsdLikeCurrency('USDT')).toBe(true);
      expect(isUsdLikeCurrency('BUSD')).toBe(true);
      expect(isUsdLikeCurrency('USDC')).toBe(true);
      expect(isUsdLikeCurrency('FDUSD')).toBe(true);
      expect(isUsdLikeCurrency('USDY')).toBe(true);
      expect(isUsdLikeCurrency('USDD')).toBe(true);
      expect(isUsdLikeCurrency('USDP')).toBe(true);
    });

    it('should return false for non-USD currencies', () => {
      expect(isUsdLikeCurrency('BTC')).toBe(false);
      expect(isUsdLikeCurrency('ETH')).toBe(false);
      expect(isUsdLikeCurrency('EUR')).toBe(false);
    });

    it('should return false for undefined', () => {
      expect(isUsdLikeCurrency(undefined)).toBe(false);
    });

    it('should be case-sensitive', () => {
      expect(isUsdLikeCurrency('usd')).toBe(false);
      expect(isUsdLikeCurrency('usdt')).toBe(false);
    });
  });

  describe('isBtcLikeCurrency', () => {
    it('should return true for BTC-like currencies', () => {
      expect(isBtcLikeCurrency('BTC')).toBe(true);
      expect(isBtcLikeCurrency('XBT')).toBe(true);
    });

    it('should return false for non-BTC currencies', () => {
      expect(isBtcLikeCurrency('ETH')).toBe(false);
      expect(isBtcLikeCurrency('USD')).toBe(false);
      expect(isBtcLikeCurrency('WBTC')).toBe(false);
    });

    it('should return false for undefined', () => {
      expect(isBtcLikeCurrency(undefined)).toBe(false);
    });
  });

  describe('isEquivalentCurrency', () => {
    it('should return true for same currency', () => {
      expect(isEquivalentCurrency('BTC', 'BTC')).toBe(true);
      expect(isEquivalentCurrency('ETH', 'ETH')).toBe(true);
    });

    it('should return true for USD equivalents', () => {
      expect(isEquivalentCurrency('USD', 'USDT')).toBe(true);
      expect(isEquivalentCurrency('USDT', 'USDC')).toBe(true);
      expect(isEquivalentCurrency('BUSD', 'FDUSD')).toBe(true);
    });

    it('should return true for BTC equivalents', () => {
      expect(isEquivalentCurrency('BTC', 'XBT')).toBe(true);
      expect(isEquivalentCurrency('XBT', 'BTC')).toBe(true);
    });

    it('should return false for different non-equivalent currencies', () => {
      expect(isEquivalentCurrency('BTC', 'ETH')).toBe(false);
      expect(isEquivalentCurrency('USD', 'EUR')).toBe(false);
      expect(isEquivalentCurrency('BTC', 'USDT')).toBe(false);
    });
  });

  describe('isBtcLikeSymbol', () => {
    it('should return true for BTC-prefixed symbols', () => {
      expect(isBtcLikeSymbol('BTCUSDT')).toBe(true);
      expect(isBtcLikeSymbol('BTCUSD')).toBe(true);
      expect(isBtcLikeSymbol('BTC-PERP')).toBe(true);
    });

    it('should return true for XBT-prefixed symbols', () => {
      expect(isBtcLikeSymbol('XBTUSD')).toBe(true);
      expect(isBtcLikeSymbol('XBTEUR')).toBe(true);
    });

    it('should return false for non-BTC symbols', () => {
      expect(isBtcLikeSymbol('ETHUSDT')).toBe(false);
      expect(isBtcLikeSymbol('SOLUSDT')).toBe(false);
    });

    it('should return false for symbols containing but not starting with BTC', () => {
      expect(isBtcLikeSymbol('WBTCUSDT')).toBe(false);
    });
  });

  describe('getCurrencySigDigits', () => {
    it('should return BTC_SIG_DIGITS for BTC-like currencies', () => {
      expect(getCurrencySigDigits('BTC')).toBe(BTC_SIG_DIGITS);
      expect(getCurrencySigDigits('XBT')).toBe(BTC_SIG_DIGITS);
      expect(getCurrencySigDigits('btc')).toBe(BTC_SIG_DIGITS); // lowercase works
    });

    it('should return USD_SIG_DIGITS for USD-like currencies', () => {
      expect(getCurrencySigDigits('USD')).toBe(USD_SIG_DIGITS);
      expect(getCurrencySigDigits('USDT')).toBe(USD_SIG_DIGITS);
      expect(getCurrencySigDigits('usdt')).toBe(USD_SIG_DIGITS); // lowercase works
    });

    it('should return DEFAULT_SIG_DIGITS for other currencies', () => {
      expect(getCurrencySigDigits('ETH')).toBe(DEFAULT_SIG_DIGITS);
      expect(getCurrencySigDigits('SOL')).toBe(DEFAULT_SIG_DIGITS);
      expect(getCurrencySigDigits('EUR')).toBe(DEFAULT_SIG_DIGITS);
    });

    it('should return DEFAULT_SIG_DIGITS for empty or null-ish input', () => {
      expect(getCurrencySigDigits('')).toBe(DEFAULT_SIG_DIGITS);
    });
  });

  describe('getCurrencyPrecision', () => {
    it('should return correct precision for BTC', () => {
      // BTC_SIG_DIGITS is 6, so precision is 0.000001
      const precision = getCurrencyPrecision('BTC');
      expect(precision).toBe(0.000001);
    });

    it('should return correct precision for USD', () => {
      // USD_SIG_DIGITS is 2, so precision is 0.01
      const precision = getCurrencyPrecision('USDT');
      expect(precision).toBe(0.01);
    });

    it('should return correct precision for other currencies', () => {
      // DEFAULT_SIG_DIGITS is 4, so precision is 0.0001
      const precision = getCurrencyPrecision('ETH');
      expect(precision).toBe(0.0001);
    });
  });

  describe('constants', () => {
    it('should have correct constant values', () => {
      expect(BTC_SIG_DIGITS).toBe(6);
      expect(USD_SIG_DIGITS).toBe(2);
      expect(DEFAULT_SIG_DIGITS).toBe(4);
    });
  });
});
