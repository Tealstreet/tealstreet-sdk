import { describe, expect, it } from 'vitest';

import { Balance, Market } from '../types';
import { getBalanceSlice } from './getBalanceSlice';

// Helper to create mock Balance
const createBalance = (overrides?: Partial<Balance>): Balance => ({
  total: 10000,
  free: 8000,
  used: 2000,
  upnl: 0,
  currencies: {
    USDT: { total: 5000, free: 4000, used: 1000, upnl: 0 },
    BTC: { total: 1, free: 0.8, used: 0.2, upnl: 0 },
  },
  ...overrides,
});

// Helper to create mock Market
const createLinearMarket = (quote = 'USDT'): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote,
    active: true,
    precision: { amount: 0.001, price: 0.01 },
    limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
    isInverse: false,
  }) as Market;

const createInverseMarket = (base = 'BTC'): Market =>
  ({
    id: 'BTCUSD',
    symbol: 'BTCUSD',
    base,
    quote: 'USD',
    active: true,
    precision: { amount: 1, price: 0.5 },
    limits: { amount: { min: 1, max: 100000 }, leverage: { min: 1, max: 100 } },
    isInverse: true,
  }) as Market;

describe('getBalanceSlice', () => {
  describe('basic functionality', () => {
    it('should return null for undefined balance', () => {
      const result = getBalanceSlice(undefined, createLinearMarket());
      expect(result).toBeNull();
    });

    it('should return full balance when market is undefined', () => {
      const balance = createBalance();
      const result = getBalanceSlice(balance, undefined);
      expect(result).toBe(balance);
    });

    it('should return currency slice for linear market', () => {
      const balance = createBalance();
      const market = createLinearMarket('USDT');
      const result = getBalanceSlice(balance, market);
      expect(result).toEqual({ total: 5000, free: 4000, used: 1000, upnl: 0 });
    });

    it('should return currency slice for inverse market', () => {
      const balance = createBalance();
      const market = createInverseMarket('BTC');
      const result = getBalanceSlice(balance, market);
      expect(result).toEqual({ total: 1, free: 0.8, used: 0.2, upnl: 0 });
    });
  });

  describe('edge cases', () => {
    it('should return full balance when margin currency is empty string', () => {
      const balance = createBalance();
      // A market where getMarginCurrency returns ''
      const market = {
        ...createLinearMarket(),
        isInverse: false,
        quote: '',
      } as Market;
      const result = getBalanceSlice(balance, market);
      expect(result).toBe(balance);
    });

    it('should return full balance when currency not found', () => {
      const balance = createBalance();
      const market = createLinearMarket('EUR'); // EUR not in currencies
      const result = getBalanceSlice(balance, market);
      expect(result).toBe(balance);
    });

    it('should return a zero slice for missing coin-margined inverse collateral', () => {
      const balance = createBalance({ info: { coinMargined: true } });
      const market = createInverseMarket('NEAR'); // NEAR not in currencies
      const result = getBalanceSlice(balance, market);
      expect(result).toEqual({ total: 0, free: 0, used: 0, upnl: 0 });
    });

    it('should handle balance with no currencies', () => {
      const balance = createBalance({ currencies: undefined });
      const market = createLinearMarket();
      const result = getBalanceSlice(balance, market);
      expect(result).toBe(balance);
    });

    it('should handle balance with empty currencies', () => {
      const balance = createBalance({ currencies: {} });
      const market = createLinearMarket();
      const result = getBalanceSlice(balance, market);
      expect(result).toBe(balance);
    });
  });
});
