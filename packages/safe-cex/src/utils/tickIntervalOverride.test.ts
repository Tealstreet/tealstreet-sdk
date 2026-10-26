import { beforeEach, describe, expect, it } from 'vitest';

import {
  ALL_EXCHANGES,
  getTickIntervalMultiplier,
  getTickIntervalMultipliersForTests,
  setTickIntervalMultipliers,
  subscribeTickIntervalMultipliers,
} from './tickIntervalOverride';

describe('tick interval override', () => {
  beforeEach(() => setTickIntervalMultipliers(undefined));

  it('notifies engine pollers after updates and removes disposed listeners', () => {
    const observed: number[] = [];
    const stop = subscribeTickIntervalMultipliers(() => observed.push(getTickIntervalMultiplier('breakoutprop')));
    setTickIntervalMultipliers({ '*': 4 });
    setTickIntervalMultipliers({ '*': 4, breakoutprop: 2 });
    setTickIntervalMultipliers(undefined);
    stop();
    setTickIntervalMultipliers({ '*': 3 });
    expect(observed).toEqual([4, 2, 1]);
  });

  it('returns 1 for an exchange with nothing set, so callers can multiply unconditionally', () => {
    expect(getTickIntervalMultiplier('bybit')).toBe(1);
  });

  it('applies a configured multiplier', () => {
    setTickIntervalMultipliers({ bybit: 4 });

    expect(getTickIntervalMultiplier('bybit')).toBe(4);
    expect(getTickIntervalMultiplier('okx')).toBe(1);
  });

  it('replaces rather than merges, so turning the flag off clears it', () => {
    setTickIntervalMultipliers({ bybit: 4 });
    setTickIntervalMultipliers({});

    expect(getTickIntervalMultiplier('bybit')).toBe(1);
  });

  /**
   * This lever exists to REDUCE request pressure during an incident. A value
   * that could also speed polling up is one bad flag away from causing the
   * outage it was reached for, so anything below 1 is neutralised rather than
   * honoured.
   */
  it('refuses to speed polling up', () => {
    setTickIntervalMultipliers({ bybit: 0.1, okx: 0, kraken: -5 });

    expect(getTickIntervalMultiplier('bybit')).toBe(1);
    expect(getTickIntervalMultiplier('okx')).toBe(1);
    expect(getTickIntervalMultiplier('kraken')).toBe(1);
  });

  it('caps so a fat-fingered value cannot effectively stop an account polling', () => {
    setTickIntervalMultipliers({ bybit: 100_000 });

    expect(getTickIntervalMultiplier('bybit')).toBe(60);
  });

  it('applies the wildcard to any exchange', () => {
    setTickIntervalMultipliers({ [ALL_EXCHANGES]: 4 });

    expect(getTickIntervalMultiplier('bybit')).toBe(4);
    expect(getTickIntervalMultiplier('some-venue-added-next-year')).toBe(4);
  });

  it('prefers an exact exchange entry over the wildcard', () => {
    setTickIntervalMultipliers({ [ALL_EXCHANGES]: 4, bybit: 10 });

    expect(getTickIntervalMultiplier('bybit')).toBe(10);
    expect(getTickIntervalMultiplier('okx')).toBe(4);
  });

  it('ignores non-finite values instead of poisoning the map', () => {
    setTickIntervalMultipliers({ bybit: Number.NaN, okx: Number.POSITIVE_INFINITY } as never);

    expect(getTickIntervalMultiplier('bybit')).toBe(1);
    // Infinity is finite-checked out, not clamped to the ceiling.
    expect(getTickIntervalMultipliersForTests()).not.toHaveProperty('bybit');
  });
});
