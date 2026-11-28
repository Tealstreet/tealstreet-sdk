// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
import type { TwapOptions } from './TwapOptions';
import type { TwapState } from './TwapState';

import { describe, expect, it } from 'vitest';

import { OrderSide } from '../../types';
import { restingSlicesNote, twapRestoreRefusal } from './twapRestoreRefusal';
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
      'its saved schedule expired with a slice still pending; check your open orders and fills before restarting it',
    );
    expect(twapRestoreRefusal(options, state({ nextOrderAt: now - 3_600_000 }), now)).toBe(
      'its saved schedule has expired',
    );
  });
});

describe('twapRestoreRefusal with resting limit slices', () => {
  const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.2 }];

  it('restores a row in its last interval, which still has limit slices to cancel', () => {
    expect(
      twapRestoreRefusal(options, state({ ordersSizes: [], ordersExecuted: 5, limitSlices }), now),
    ).toBeUndefined();
    expect(twapRestoreRefusal(options, state({ ordersSizes: [], ordersExecuted: 5 }), now)).toBe('no slices remain');
  });

  it('names each resting slice for a refusal message', () => {
    expect(restingSlicesNote(state({ side: OrderSide.Sell, limitSlices }))).toBe(
      '. Its limit orders are still open: sell 0.2 at 100 (order o1)',
    );
    expect(restingSlicesNote(state())).toBe('');
  });
});
