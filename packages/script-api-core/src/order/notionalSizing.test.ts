import type { Market } from '@tealstreet/safe-cex/src/types';

import { describe, expect, it } from 'vitest';

import { notionalToNativeSize } from './notionalSizing';

const market = (overrides: Partial<Market>): Market =>
  ({ symbol: 'X', contractSize: 1, isInverse: false, ...overrides }) as Market;

describe('notionalToNativeSize', () => {
  it('is notional / price on an ordinary 1:1 linear market', () => {
    expect(notionalToNativeSize(1000, market({}), 50_000)).toBe(0.02);
  });

  it('divides by ctVal on a linear market that publishes one', () => {
    // OKX DOGE-USDT-SWAP: 1000 DOGE per contract. $1000 at $0.10 is 10,000
    // DOGE — which is 10 contracts, not 10,000. Sending the base amount is a
    // 1000x overshoot.
    expect(notionalToNativeSize(1000, market({ contractSize: 1000 }), 0.1)).toBe(10);
  });

  it('divides by ctVal the other way when ctVal is below 1', () => {
    // OKX BTC-USDT-SWAP: 0.01 BTC per contract.
    expect(notionalToNativeSize(1000, market({ contractSize: 0.01 }), 50_000)).toBe(2);
  });

  it('ignores the price entirely on an inverse market', () => {
    // A contract IS a fixed quote amount there, so $1000 of a $100-per-contract
    // inverse market is 10 contracts at any price.
    const inverse = market({ isInverse: true, contractSize: 100 });
    expect(notionalToNativeSize(1000, inverse, 50_000)).toBe(10);
    expect(notionalToNativeSize(1000, inverse, 1)).toBe(10);
  });

  it('treats a missing or zero contractSize as 1', () => {
    expect(notionalToNativeSize(1000, market({ contractSize: 0 }), 50_000)).toBe(0.02);
    expect(notionalToNativeSize(1000, market({ contractSize: undefined }), 50_000)).toBe(0.02);
  });
});
