import { describe, expect, it } from 'vitest';

import { nativePositionSize } from './nativePositionSize';

/**
 * The case this function exists to make impossible to get wrong twice.
 *
 * `getPositionNativeContracts` returned `extra.contractsNative` outright while
 * `getPositionNativeSize` took the larger of it and the derived value. On a
 * stale extra the two disagreed by 4x, and the disagreeing one fed the Agent's
 * percentage closes and the full-close check.
 */

describe('nativePositionSize', () => {
  it('divides an inverse position by contractSize, ignoring price', () => {
    expect(nativePositionSize({ contracts: 0, notional: 40 }, { contractSize: 100, isInverse: true })).toBe(0.4);
  });

  it('divides a linear position by contractSize', () => {
    // OKX DOGE-USDT-SWAP: 1000 DOGE per contract.
    expect(nativePositionSize({ contracts: 5000 }, { contractSize: 1000 })).toBe(5);
  });

  it('is the plain contract count on an ordinary 1:1 market', () => {
    expect(nativePositionSize({ contracts: 0.25 }, { contractSize: 1 })).toBe(0.25);
  });

  it('prefers the venue-supplied native size when it is the larger', () => {
    expect(
      nativePositionSize({ contracts: 0, notional: 40, extra: { contractsNative: 0.9 } }, {
        contractSize: 100,
        isInverse: true,
      }),
    ).toBe(0.9);
  });

  it('IGNORES a stale extra that is smaller than the derived size', () => {
    // The whole bug: 0.1 reported for a 0.4-contract position, so a "close
    // 100%" closed a quarter and reported success.
    expect(
      nativePositionSize({ contracts: 0, notional: 40, extra: { contractsNative: 0.1 } }, {
        contractSize: 100,
        isInverse: true,
      }),
    ).toBe(0.4);
  });

  it('falls back to the raw count when contractSize is missing or zero', () => {
    expect(nativePositionSize({ contracts: 3 }, {})).toBe(3);
    expect(nativePositionSize({ contracts: 3 }, { contractSize: 0 })).toBe(3);
  });

  it('is always positive, whichever side the position is', () => {
    expect(nativePositionSize({ contracts: -5000 }, { contractSize: 1000 })).toBe(5);
  });
});
