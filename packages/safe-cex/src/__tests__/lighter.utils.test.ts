import type { Market, Order } from '../types';

import { describe, expect, it } from 'vitest';

import {
  getLighterAmountDecimals,
  getLighterOrderMinimums,
  getLighterPriceDecimals,
  getPrecision,
  isLighterL1Address,
  parseLighterApiKeyIndex,
  parseLighterPositiveIntegerString,
  resolveLighterCancelTargets,
  toLighterApiKeyIndexString,
  toLighterScaledInt,
} from '../exchanges/lighter/lighter.utils';

const mockOrder = (id: string, symbol: string): Order =>
  ({
    id,
    symbol,
  }) as Order;

const mockMarket = (id: string, symbol: string): Market =>
  ({
    id,
    symbol,
  }) as Market;

describe('resolveLighterCancelTargets', () => {
  it('resolves orderId and marketId from provided orders', () => {
    const orders: Order[] = [mockOrder('101', 'BTCUSDT'), mockOrder('102', 'ETHUSDT')];
    const marketsBySymbol: Record<string, Market> = {
      BTCUSDT: mockMarket('0', 'BTCUSDT'),
      ETHUSDT: mockMarket('1', 'ETHUSDT'),
    };

    expect(resolveLighterCancelTargets(orders, marketsBySymbol)).toEqual([
      { orderId: '101', marketId: 0 },
      { orderId: '102', marketId: 1 },
    ]);
  });

  it('skips orders that do not have market metadata', () => {
    const orders: Order[] = [mockOrder('201', 'BTCUSDT'), mockOrder('202', 'SOLUSDT')];
    const marketsBySymbol: Record<string, Market> = {
      BTCUSDT: mockMarket('0', 'BTCUSDT'),
    };

    expect(resolveLighterCancelTargets(orders, marketsBySymbol)).toEqual([{ orderId: '201', marketId: 0 }]);
  });

  it('skips markets with invalid numeric market id', () => {
    const orders: Order[] = [mockOrder('301', 'BTCUSDT')];
    const marketsBySymbol: Record<string, Market> = {
      BTCUSDT: mockMarket('not-a-number', 'BTCUSDT'),
    };

    expect(resolveLighterCancelTargets(orders, marketsBySymbol)).toEqual([]);
  });
});

describe('lighter precision utilities', () => {
  it('handles scientific notation in precision extraction', () => {
    expect(getPrecision(1e-8)).toBe(8);
  });

  it('uses market precision fallback for amount/price decimals', () => {
    const market = {
      precision: {
        amount: 0.001,
        price: 0.01,
      },
    } as Market;

    expect(getLighterAmountDecimals(market)).toBe(3);
    expect(getLighterPriceDecimals(market)).toBe(2);
  });

  it('derives decimals from power-of-ten scale fields', () => {
    const market = {
      precision: {
        amount: 1,
        price: 1,
      },
      info: {
        sizeScale: 1_000_000,
        priceScale: 100,
      },
    } as Market;

    expect(getLighterAmountDecimals(market)).toBe(6);
    expect(getLighterPriceDecimals(market)).toBe(2);
  });

  it('scales decimal numbers into deterministic bigint units', () => {
    expect(toLighterScaledInt(1.234, 3)).toBe(1234n);
    expect(toLighterScaledInt(40.12, 2)).toBe(4012n);
  });

  it('extracts order minimums from exchange market info', () => {
    const market = {
      info: {
        min_base_amount: '20.0',
        min_quote_amount: '10.000000',
      },
    } as Market;

    expect(getLighterOrderMinimums(market)).toEqual({
      minBaseAmount: 20,
      minQuoteAmount: 10,
    });
  });

  it('returns zero minimums when metadata is missing or invalid', () => {
    const market = {
      info: {
        min_base_amount: 'not-a-number',
        min_quote_amount: '-5',
      },
    } as Market;

    expect(getLighterOrderMinimums(market)).toEqual({
      minBaseAmount: 0,
      minQuoteAmount: 0,
    });
  });
});

describe('lighter api key index parsing', () => {
  it('parses integer values from string and number inputs', () => {
    expect(parseLighterApiKeyIndex('2')).toBe(2);
    expect(parseLighterApiKeyIndex(3)).toBe(3);
    expect(parseLighterApiKeyIndex('2 (Desktop)')).toBe(2);
  });

  it('returns undefined for non-numeric values', () => {
    expect(parseLighterApiKeyIndex(undefined)).toBeUndefined();
    expect(parseLighterApiKeyIndex('')).toBeUndefined();
    expect(parseLighterApiKeyIndex('not-a-number')).toBeUndefined();
  });

  it('formats a safe api key index string with fallback', () => {
    expect(toLighterApiKeyIndexString('5')).toBe('5');
    expect(toLighterApiKeyIndexString('5 (Desktop)')).toBe('5');
    expect(toLighterApiKeyIndexString('abc', '0')).toBe('0');
  });

  it('parses positive integer strings for account index values', () => {
    expect(parseLighterPositiveIntegerString('  42  ')).toBe('42');
    expect(parseLighterPositiveIntegerString('00042')).toBe('42');
    expect(parseLighterPositiveIntegerString('account-7')).toBeUndefined();
    expect(parseLighterPositiveIntegerString('none')).toBeUndefined();
  });

  it('detects valid L1 address shape', () => {
    expect(isLighterL1Address('0x70997970C51812dc3A010C7d01b50e0d17dc79C8')).toBe(true);
    expect(isLighterL1Address('70997970C51812dc3A010C7d01b50e0d17dc79C8')).toBe(false);
    expect(isLighterL1Address('e33b8a8a8034')).toBe(false);
  });
});
