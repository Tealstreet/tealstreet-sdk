import { describe, expect, it } from 'vitest';

import { OrderSide, OrderStatus, OrderType, type Market, type Order } from '@tealstreet/safe-cex/src/types';
import { formatBaseSize, formatOrderBaseSize } from './formatBaseSize';

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

const orderFixture = (overrides: Partial<Order>): Order => ({
  id: 'partial',
  status: OrderStatus.Open,
  symbol: 'BTCUSD',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 0,
  amount: 7,
  filled: 2,
  remaining: 5,
  reduceOnly: false,
  lastUpdate: 0,
  ...overrides,
});

describe('formatBaseSize', () => {
  it.each([
    { name: 'OKX BTC', size: 0.0012, market: market(0.01, 0.01), expected: '0.0012' },
    { name: 'Blofin BTC', size: 0.0012, market: market(0.1, 0.001), expected: '0.0012' },
    { name: 'OKX DOGE', size: 240, market: market(0.01, 1000), expected: '240' },
    { name: 'inverse', size: 0.0006, market: market(1, 100, true), expected: '0.0006' },
    { name: 'one-dollar inverse BTC', size: 0.00001, market: market(1, 1, true), expected: '0.00001' },
    { name: 'tiny inverse short', size: -0.00000001, market: market(1, 1, true), expected: '0.00000001' },
    { name: 'price-derived inverse size', size: 1 / 70000, market: market(1, 1, true), expected: '0.000014285714' },
    { name: 'flat inverse', size: 0, market: market(1, 1, true), expected: '0' },
    { name: 'large inverse', size: 15000, market: market(1, 1, true), expected: '15.0k' },
    { name: 'short', size: -0.0012, market: market(0.01, 0.01), expected: '0.0012' },
    { name: 'no market', size: 0.0006, market: undefined, expected: '0.0006' },
  ])('formats $name with the base lot step', ({ size, market: m, expected }) => {
    expect(formatBaseSize(size, m)).toBe(expected);
  });
});

describe('formatOrderBaseSize', () => {
  it.each([
    { name: 'native contract remainder', remaining: 5, price: 50_000, notional: 500, expected: '0.01' },
    { name: 'base remainder', remaining: 0.01, price: 50_000, notional: 500, expected: '0.01' },
    { name: 'order price rather than market price', remaining: 5, price: 25_000, notional: 500, expected: '0.02' },
    { name: 'tiny inverse remainder', remaining: 1, price: 100_000, notional: 1, expected: '0.00001' },
    { name: 'closed remainder', remaining: 0, price: 50_000, notional: 0, expected: '0' },
    { name: 'missing price', remaining: 5, price: 0, notional: 500, expected: '-' },
    { name: 'invalid price', remaining: 5, price: NaN, notional: 500, expected: '-' },
    { name: 'infinite price', remaining: 5, price: Infinity, notional: 500, expected: '-' },
    { name: 'invalid notional', remaining: 5, price: 50_000, notional: NaN, expected: '-' },
  ])('formats $name without claiming a native count is base', ({ remaining, price, notional, expected }) => {
    const order = orderFixture({ remaining, price, extra: { remainingNotional: notional } });
    expect(formatOrderBaseSize(order, market(1, 100, true))).toBe(expected);
  });

  it('preserves inverse base remainders without native metadata', () => {
    expect(formatOrderBaseSize(orderFixture({ remaining: 0.01 }), market(1, 100, true))).toBe('0.01');
  });

  it('keeps linear base remainders even when notional metadata is present', () => {
    const order = orderFixture({ remaining: 0.0012, price: 50_000, extra: { remainingNotional: 500 } });
    expect(formatOrderBaseSize(order, market(0.01, 0.01))).toBe('0.0012');
  });
});
