import type { TwapOptions } from './TwapOptions';
import type { TwapState } from './TwapState';

import { describe, expect, it } from 'vitest';

import { OrderSide } from '../../types';
import { twapRestoreRefusal } from './twapRestoreRefusal';
import { TwapStatus } from './TwapStatus';

const now = 1_800_000_000_000;
const options = { accountName: 'a', size: 1, duration: 10, ordersCount: 5, randomnessPercentage: 0 } as TwapOptions;
const state = (overrides: Partial<TwapState> = {}): TwapState =>
  ({
    accountName: 'a',
    size: 1,
    side: OrderSide.Buy,
    status: TwapStatus.Running,
    ordersSizes: [0.2, 0.2],
    ordersCount: 5,
    ordersExecuted: 2,
    nextOrderAt: now,
    ...overrides,
  }) as TwapState;

describe('twapRestoreRefusal with a slice in flight', () => {
  it('restores a consistent row whose last slice is in flight', () => {
    expect(twapRestoreRefusal(options, state({ ordersSizes: [], inFlight: { size: 0.2 } }), now)).toBeUndefined();
    expect(twapRestoreRefusal(options, state({ inFlight: { size: 0.2, clientOrderId: 'c' } }), now)).toBeUndefined();
  });

  it.each([Number.NaN, -0.1, 0, Number.POSITIVE_INFINITY])('refuses an in-flight size of %s', (size) => {
    expect(twapRestoreRefusal(options, state({ inFlight: { size } }), now)).toBe('its in-flight slice is invalid');
  });

  it('counts the in-flight slice toward the TWAP size', () => {
    expect(twapRestoreRefusal(options, state({ inFlight: { size: 0.7 } }), now)).toBe(
      'its remaining slices exceed the TWAP size',
    );
  });

  it('counts the in-flight slice toward ordersCount', () => {
    expect(twapRestoreRefusal(options, state({ ordersExecuted: 3, inFlight: { size: 0.1 } }), now)).toBe(
      'its remaining and executed slices exceed ordersCount',
    );
  });

  it('says a slice may have been in flight when its schedule lapsed', () => {
    const lapsed = state({ nextOrderAt: now - 3_600_000, inFlight: { size: 0.2 } });
    expect(twapRestoreRefusal(options, lapsed, now)).toBe(
      'its schedule lapsed, so no tab was running it; a slice may have been in flight, check open orders and fills',
    );
    expect(twapRestoreRefusal(options, state({ nextOrderAt: now - 3_600_000 }), now)).toBe(
      'its schedule lapsed, so no tab was running it',
    );
  });
});
