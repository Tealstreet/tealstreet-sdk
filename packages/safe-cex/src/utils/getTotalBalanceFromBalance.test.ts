import { describe, expect, it } from 'vitest';

import { Balance, Market } from '../types';
import { getTotalBalanceFromBalance } from './getTotalBalanceFromBalance';

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

describe('getTotalBalanceFromBalance', () => {
  describe('basic functionality', () => {
    it('should return 0 for undefined balance', () => {
      const result = getTotalBalanceFromBalance(undefined, createLinearMarket());
      expect(result).toBe(0);
    });

    it('should return total balance when market is undefined', () => {
      const balance = createBalance();
      const result = getTotalBalanceFromBalance(balance, undefined);
      expect(result).toBe(10000);
    });

    it('should return currency total for linear market', () => {
      const balance = createBalance();
      const market = createLinearMarket('USDT');
      const result = getTotalBalanceFromBalance(balance, market);
      expect(result).toBe(5000);
    });

    it('should return currency total for inverse market', () => {
      const balance = createBalance();
      const market = createInverseMarket('BTC');
      const result = getTotalBalanceFromBalance(balance, market);
      expect(result).toBe(1);
    });
  });

  describe('edge cases', () => {
    it('should return 0 when balance total is undefined', () => {
      const balance = createBalance({ total: undefined as any });
      const result = getTotalBalanceFromBalance(balance, undefined);
      expect(result).toBe(0);
    });

    it('should return 0 when balance total is 0', () => {
      const balance = createBalance({ total: 0 });
      const result = getTotalBalanceFromBalance(balance, undefined);
      expect(result).toBe(0);
    });

    it('should return full balance total when currency not found', () => {
      const balance = createBalance();
      const market = createLinearMarket('EUR');
      const result = getTotalBalanceFromBalance(balance, market);
      expect(result).toBe(10000);
    });

    it('should return 0 when currency slice has no total', () => {
      const balance = createBalance({
        currencies: {
          USDT: { free: 4000, used: 1000 } as any,
        },
      });
      const market = createLinearMarket('USDT');
      const result = getTotalBalanceFromBalance(balance, market);
      expect(result).toBe(0);
    });
  });
});
