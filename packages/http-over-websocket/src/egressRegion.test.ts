import { describe, expect, it } from 'vitest';

import { isTokyoEgress } from './egressRegion';

describe('isTokyoEgress', () => {
  it('matches Tokyo exchange names and request URLs', () => {
    expect(isTokyoEgress('binance')).toBe(true);
    expect(isTokyoEgress('https://fapi.binance.com')).toBe(true);
    expect(isTokyoEgress('https://api.bybit.com')).toBe(false);
  });

  it('does not coerce unexpected values through RegExp.test', () => {
    const recursiveValue = {
      toString(): string {
        return recursiveValue.toString();
      },
    };

    expect(isTokyoEgress(recursiveValue)).toBe(false);
    expect(isTokyoEgress(undefined)).toBe(false);
  });
});
