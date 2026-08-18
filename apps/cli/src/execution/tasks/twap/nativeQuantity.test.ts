import { describe, expect, it } from 'vitest';

import { twapTotalInNativeContracts } from './nativeQuantity';

describe('twapTotalInNativeContracts', () => {
  it('converts a bare base quantity at the legacy engine boundary', () => {
    expect(twapTotalInNativeContracts({ contractSize: 0.01 }, 1, false)).toBe(100);
  });

  it('preserves a size already expressed in venue-native contracts', () => {
    expect(twapTotalInNativeContracts({ contractSize: 0.01 }, 1, true)).toBe(1);
  });

  it('rejects bare base quantities for inverse legacy TWAPs', () => {
    expect(() => twapTotalInNativeContracts({ contractSize: 100, isInverse: true }, 1, false)).toThrow(
      'Bare base quantities are not supported for inverse TWAP markets',
    );
  });

  it('fails closed on an invalid published contract size', () => {
    expect(() => twapTotalInNativeContracts({ contractSize: 0 }, 1, false)).toThrow('invalid contract size');
  });
});
