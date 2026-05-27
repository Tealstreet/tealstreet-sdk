import { describe, it, expect } from 'vitest';
import {
  convertFromQuoteSizeToNative,
  convertFromBaseSizeToNative,
  convertFromNativeSizeToQuote,
  convertFromNativeSizeToBase,
  convertMarginToSizeCurrency,
  convertSizeToMarginCurrency,
  convertFromMarginToQuote,
  getSizeCurrency,
  getMarginCurrency,
  getContractType,
} from './sizing';
import { Market } from '../types';

// Helper to create mock markets
const createLinearMarket = (overrides?: Partial<Market>): Market => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: {
    amount: { min: 0.001, max: 1000 },
    leverage: { min: 1, max: 100 },
  },
  isInverse: false,
  ...overrides,
});

const createInverseMarket = (overrides?: Partial<Market>): Market => ({
  id: 'XBTUSD',
  symbol: 'XBTUSD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 1, price: 0.5 },
  limits: {
    amount: { min: 1, max: 10000000 },
    leverage: { min: 1, max: 100 },
  },
  isInverse: true,
  ...overrides,
});

const createQuantoMarket = (overrides?: Partial<Market>): Market => ({
  id: 'ETHUSD',
  symbol: 'ETHUSD',
  base: 'ETH',
  quote: 'USD',
  active: true,
  precision: { amount: 1, price: 0.05 },
  limits: {
    amount: { min: 1, max: 10000000 },
    leverage: { min: 1, max: 100 },
  },
  isInverse: false,
  info: { isQuanto: true },
  ...overrides,
});

describe('sizing', () => {
  describe('convertFromQuoteSizeToNative', () => {
    it('should convert quote size to native for linear market', () => {
      const market = createLinearMarket();
      // $50,000 USDT at $50,000 BTC price = 1 BTC
      const result = convertFromQuoteSizeToNative(50000, market, 50000);
      expect(result).toBe(1);
    });

    it('should convert quote size to native for inverse market', () => {
      const market = createInverseMarket();
      // $50,000 USD contracts / 1 contract size = 50000 contracts
      const result = convertFromQuoteSizeToNative(50000, market, 50000);
      expect(result).toBe(50000);
    });

    it('should respect contractSize for linear market', () => {
      const market = createLinearMarket({ contractSize: 0.01 });
      // $50,000 USDT at $50,000 price = 1 BTC / 0.01 contract = 100 contracts
      const result = convertFromQuoteSizeToNative(50000, market, 50000);
      expect(result).toBe(100);
    });

    it('should adjust to market precision', () => {
      const market = createLinearMarket({ precision: { amount: 0.01, price: 0.01 } });
      // $1000 at $40000 = 0.025 BTC, adjusted to 0.03
      const result = convertFromQuoteSizeToNative(1000, market, 40000);
      expect(result).toBe(0.03);
    });
  });

  describe('convertFromBaseSizeToNative', () => {
    it('should convert base size to native for linear market', () => {
      const market = createLinearMarket();
      // 1 BTC with linear market = 1 native
      const result = convertFromBaseSizeToNative(1, market, 50000);
      expect(result).toBe(1);
    });

    it('should convert base size to native for inverse market', () => {
      const market = createInverseMarket();
      // 1 BTC * $50,000 = $50,000 contracts
      const result = convertFromBaseSizeToNative(1, market, 50000);
      expect(result).toBe(50000);
    });

    it('should respect contractSize for inverse market', () => {
      const market = createInverseMarket({ contractSize: 100 });
      // 1 BTC * $50,000 / 100 = 500 contracts
      const result = convertFromBaseSizeToNative(1, market, 50000);
      expect(result).toBe(500);
    });
  });

  describe('convertFromNativeSizeToQuote', () => {
    it('should convert native to quote for linear market', () => {
      const market = createLinearMarket();
      // 1 BTC * $50,000 = $50,000 USDT
      const result = convertFromNativeSizeToQuote(1, market, 50000);
      expect(result).toBe(50000);
    });

    it('should convert native to quote for inverse market', () => {
      const market = createInverseMarket();
      // 50000 contracts * 1 = $50,000
      const result = convertFromNativeSizeToQuote(50000, market, 50000);
      expect(result).toBe(50000);
    });

    it('should respect contractSize', () => {
      const market = createLinearMarket({ contractSize: 0.01 });
      // 100 contracts * 0.01 = 1 BTC * $50,000 = $50,000
      const result = convertFromNativeSizeToQuote(100, market, 50000);
      expect(result).toBe(50000);
    });
  });

  describe('convertFromNativeSizeToBase', () => {
    it('should convert native to base for linear market', () => {
      const market = createLinearMarket();
      // 1 native = 1 BTC for linear
      const result = convertFromNativeSizeToBase(1, market, 50000);
      expect(result).toBe(1);
    });

    it('should convert native to base for inverse market', () => {
      const market = createInverseMarket();
      // $50,000 contracts / $50,000 = 1 BTC
      const result = convertFromNativeSizeToBase(50000, market, 50000);
      expect(result).toBe(1);
    });

    it('should respect contractSize for inverse market', () => {
      const market = createInverseMarket({ contractSize: 100 });
      // 500 contracts * $100 / $50,000 = 1 BTC
      const result = convertFromNativeSizeToBase(500, market, 50000);
      expect(result).toBe(1);
    });

    it('should respect contractSize for linear market', () => {
      const market = createLinearMarket({ contractSize: 0.01 });
      // 100 contracts * 0.01 = 1 BTC
      const result = convertFromNativeSizeToBase(100, market, 50000);
      expect(result).toBe(1);
    });
  });

  describe('convertMarginToSizeCurrency', () => {
    it('should convert margin to size for linear market', () => {
      const market = createLinearMarket();
      // $1000 USDT margin at $50,000 = 0.02 BTC
      const result = convertMarginToSizeCurrency(1000, market, 50000);
      expect(result).toBe(0.02);
    });

    it('should convert margin to size for inverse market', () => {
      const market = createInverseMarket();
      // 1 BTC margin * $50,000 = 50000 contracts
      const result = convertMarginToSizeCurrency(1, market, 50000);
      expect(result).toBe(50000);
    });

    it('should handle quanto market with btcPrice', () => {
      const market = createQuantoMarket();
      // 1 BTC margin * $40,000 btcPrice = $40,000 margin in quote
      // $40,000 / $2000 ETH price = 20 ETH (adjusted to precision 1)
      const result = convertMarginToSizeCurrency(1, market, 2000, 40000);
      expect(result).toBe(20);
    });

    it('should return 0 for quanto market without btcPrice', () => {
      const market = createQuantoMarket();
      const result = convertMarginToSizeCurrency(1, market, 2000);
      expect(result).toBe(0);
    });
  });

  describe('convertSizeToMarginCurrency', () => {
    it('should convert size to margin for linear market', () => {
      const market = createLinearMarket();
      // 0.02 BTC * $50,000 = $1000 USDT
      const result = convertSizeToMarginCurrency(0.02, market, 50000);
      expect(result).toBe(1000);
    });

    it('should convert size to margin for inverse market', () => {
      const market = createInverseMarket();
      // 50000 contracts / $50,000 = 1 BTC
      const result = convertSizeToMarginCurrency(50000, market, 50000);
      expect(result).toBe(1);
    });

    it('should handle quanto market with btcPrice', () => {
      const market = createQuantoMarket();
      // 20 contracts * $2000 = $40,000 / $40,000 btcPrice = 1 BTC
      const result = convertSizeToMarginCurrency(20, market, 2000, 40000);
      expect(result).toBe(1);
    });

    it('should return 0 for quanto market without btcPrice', () => {
      const market = createQuantoMarket();
      const result = convertSizeToMarginCurrency(20, market, 2000);
      expect(result).toBe(0);
    });
  });

  describe('convertFromMarginToQuote', () => {
    it('should return margin as-is for linear market', () => {
      const market = createLinearMarket();
      // Linear markets are already in quote currency
      expect(convertFromMarginToQuote(1000, market)).toBe(1000);
    });

    it('should convert margin to quote for inverse market', () => {
      const market = createInverseMarket();
      // 1 BTC * $50,000 = $50,000
      expect(convertFromMarginToQuote(1, market, 50000)).toBe(50000);
    });

    it('should return 0 for inverse market without price', () => {
      const market = createInverseMarket();
      expect(convertFromMarginToQuote(1, market)).toBe(0);
    });

    it('should convert margin to quote for quanto market', () => {
      const market = createQuantoMarket();
      // 1 BTC * $40,000 btcPrice = $40,000
      expect(convertFromMarginToQuote(1, market, undefined, 40000)).toBe(40000);
    });

    it('should return 0 for quanto market without btcPrice', () => {
      const market = createQuantoMarket();
      expect(convertFromMarginToQuote(1, market)).toBe(0);
    });
  });

  describe('getSizeCurrency', () => {
    it('should return base currency for linear market', () => {
      const market = createLinearMarket();
      expect(getSizeCurrency(market)).toBe('BTC');
    });

    it('should return quote currency for inverse market', () => {
      const market = createInverseMarket();
      expect(getSizeCurrency(market)).toBe('USD');
    });

    it('should return "Contracts" when contractSize is not 1', () => {
      const market = createLinearMarket({ contractSize: 0.01 });
      expect(getSizeCurrency(market)).toBe('Contracts');
    });

    it('should return empty string for undefined market', () => {
      expect(getSizeCurrency(undefined)).toBe('');
    });
  });

  describe('getMarginCurrency', () => {
    it('should return quote currency for linear market', () => {
      const market = createLinearMarket();
      expect(getMarginCurrency(market)).toBe('USDT');
    });

    it('should return base currency for inverse market', () => {
      const market = createInverseMarket();
      expect(getMarginCurrency(market)).toBe('BTC');
    });

    it('should return "BTC" for quanto market', () => {
      const market = createQuantoMarket();
      expect(getMarginCurrency(market)).toBe('BTC');
    });

    it('should return empty string for spot market', () => {
      const market = createLinearMarket({ isSpot: true });
      expect(getMarginCurrency(market)).toBe('');
    });

    it('should return fallback for undefined market', () => {
      expect(getMarginCurrency(undefined, 'USD')).toBe('USD');
    });

    it('should return empty string for undefined market with no fallback', () => {
      expect(getMarginCurrency(undefined)).toBe('');
    });
  });

  describe('getContractType', () => {
    it('should return "Linear" for linear market', () => {
      const market = createLinearMarket();
      expect(getContractType(market)).toBe('Linear');
    });

    it('should return "Inverse" for inverse market', () => {
      const market = createInverseMarket();
      expect(getContractType(market)).toBe('Inverse');
    });

    it('should return "Quanto" for quanto market', () => {
      const market = createQuantoMarket();
      expect(getContractType(market)).toBe('Quanto');
    });

    it('should return "Linear" for undefined market', () => {
      expect(getContractType(undefined)).toBe('Linear');
    });
  });
});
