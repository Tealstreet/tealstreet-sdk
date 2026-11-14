import { describe, expect, it } from 'vitest';

import { nativePositionSize } from './nativePositionSize';

describe('native position size evidence', () => {
  it.each([0.1, 0.9])('refuses conflicting native evidence %s instead of choosing a maximum', (native) => {
    expect(
      nativePositionSize(
        { contracts: 0, notional: 40, extra: { contractsNative: native } },
        { isInverse: true, contractSize: 100 },
      ),
    ).toBeNull();
  });
  it('uses consistent native evidence without changing its value', () => {
    const native = 0.1 + 0.2;
    expect(nativePositionSize({ contracts: 0.3, extra: { contractsNative: native } }, {})).toBe(native);
  });
  it('uses native evidence when the derived quantity is unavailable', () => {
    expect(nativePositionSize({ extra: { contractsNative: 3 } }, {})).toBe(3);
  });
  it('derives linear quantity when native is absent', () => {
    expect(nativePositionSize({ contracts: -5000 }, { contractSize: 1000 })).toBe(5);
  });
  it('derives inverse quantity when native is absent', () => {
    expect(nativePositionSize({ notional: -40 }, { isInverse: true, contractSize: 100 })).toBe(0.4);
  });
  it.each([{}, { contracts: NaN }, { contracts: Infinity }])('preserves unknown derived quantity: %j', (position) => {
    expect(nativePositionSize(position, {})).toBeNull();
  });
  it('keeps known flat distinct from unknown', () => {
    expect(nativePositionSize({ contracts: 0, extra: { contractsNative: 0 } }, {})).toBe(0);
    expect(nativePositionSize({ contracts: 1, extra: { contractsNative: 0 } }, {})).toBeNull();
  });
  it.each([NaN, Infinity, -1])('refuses unusable contract size %s', (contractSize) => {
    expect(nativePositionSize({ contracts: 3 }, { contractSize })).toBeNull();
  });
  it('does not replace invalid native evidence with a guessed size', () => {
    expect(nativePositionSize({ contracts: 3, extra: { contractsNative: NaN } }, {})).toBeNull();
  });
});
