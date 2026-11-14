import { describe, expect, it } from 'vitest';

import { nativePositionSize } from './positionSizing';

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

  it('refuses a native size larger than conflicting derived evidence', () => {
    expect(
      nativePositionSize(
        { contracts: 0, notional: 40, extra: { contractsNative: 0.9 } },
        {
          contractSize: 100,
          isInverse: true,
        },
      ),
    ).toBeNull();
  });

  it('refuses stale native evidence instead of selecting larger derived evidence', () => {
    expect(
      nativePositionSize(
        { contracts: 0, notional: 40, extra: { contractsNative: 0.1 } },
        {
          contractSize: 100,
          isInverse: true,
        },
      ),
    ).toBeNull();
  });

  it('falls back to the raw count when contractSize is missing or zero', () => {
    expect(nativePositionSize({ contracts: 3 }, {})).toBe(3);
    expect(nativePositionSize({ contracts: 3 }, { contractSize: 0 })).toBe(3);
  });

  it('is always positive, whichever side the position is', () => {
    expect(nativePositionSize({ contracts: -5000 }, { contractSize: 1000 })).toBe(5);
  });
});
