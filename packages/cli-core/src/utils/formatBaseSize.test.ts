import { describe, expect, it } from 'vitest';

import type { Market } from '@tealstreet/safe-cex/src/types';
import { formatBaseSize } from './formatBaseSize';

const market = (amountStep: number, contractSize: number, isInverse = false): Market => ({
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: amountStep, price: 0.1 },
  limits: { amount: { min: amountStep, max: 100 }, leverage: { min: 1, max: 100 } },
  contractSize,
  isInverse,
});

describe('formatBaseSize', () => {
  it.each([
    { name: 'OKX BTC', size: 0.0012, market: market(0.01, 0.01), expected: '0.0012' },
    { name: 'Blofin BTC', size: 0.0012, market: market(0.1, 0.001), expected: '0.0012' },
    { name: 'OKX DOGE', size: 240, market: market(0.01, 1000), expected: '240' },
    { name: 'inverse', size: 0.0006, market: market(1, 100, true), expected: '0.0006' },
    { name: 'short', size: -0.0012, market: market(0.01, 0.01), expected: '0.0012' },
    { name: 'no market', size: 0.0006, market: undefined, expected: '0.0006' },
  ])('formats $name with the base lot step', ({ size, market: m, expected }) => {
    expect(formatBaseSize(size, m)).toBe(expected);
  });
});
