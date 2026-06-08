import type { DisplayedPosition, ExchangeName, ExchangeOptions, StoreData, Writable } from '../../types';

import { describe, expect, it } from 'vitest';

import { PositionSide } from '../../types';
import { DisplayedMarginPostprocessor } from './DisplayedMarginPostprocessor';

const pos = (side: PositionSide, notional: number | undefined): DisplayedPosition =>
  ({ side, notional }) as DisplayedPosition;

const createStore = (displayedPositions: DisplayedPosition[]): Writable<StoreData> =>
  ({ displayedPositions }) as Writable<StoreData>;

const createProcessor = () => new DisplayedMarginPostprocessor('test-account', 'bybit' as ExchangeName, {} as ExchangeOptions);

/**
 * Reference implementation: the exact formula the margin UI used inline before
 * the hoist (filter+reduce per side, separate total loop). The processor must
 * reproduce this byte-for-byte.
 */
const oracle = (positions: DisplayedPosition[]) => {
  const longExposure = positions
    .filter((p) => p.side === PositionSide.Long)
    .reduce((sum, p) => sum + Math.abs(p.notional || 0), 0);
  const shortExposure = positions
    .filter((p) => p.side === PositionSide.Short)
    .reduce((sum, p) => sum + Math.abs(p.notional || 0), 0);
  let totalNotional = 0;
  for (const p of positions) totalNotional += Math.abs(p.notional || 0);
  return { longExposure, shortExposure, totalNotional };
};

const L = PositionSide.Long;
const S = PositionSide.Short;

describe('DisplayedMarginPostprocessor', () => {
  it('writes zeros for no positions', () => {
    const store = createStore([]);
    createProcessor().process(store);
    expect(store.displayedMargin).toEqual({ longExposure: 0, shortExposure: 0, totalNotional: 0 });
  });

  it('aggregates a simple long/short mix', () => {
    const store = createStore([pos(L, 100), pos(S, 40), pos(L, 10)]);
    createProcessor().process(store);
    expect(store.displayedMargin).toEqual({ longExposure: 110, shortExposure: 40, totalNotional: 150 });
  });

  it('uses absolute notional and treats missing notional as 0', () => {
    const store = createStore([pos(L, -100), pos(S, undefined), pos(L, 0)]);
    createProcessor().process(store);
    expect(store.displayedMargin).toEqual({ longExposure: 100, shortExposure: 0, totalNotional: 100 });
  });

  it('counts an unknown side toward totalNotional only (matches old behavior)', () => {
    const store = createStore([pos(L, 100), pos('flat' as PositionSide, 25)]);
    createProcessor().process(store);
    expect(store.displayedMargin).toEqual(oracle(store.displayedPositions));
    expect(store.displayedMargin).toEqual({ longExposure: 100, shortExposure: 0, totalNotional: 125 });
  });

  it('handles a guarded missing displayedPositions array', () => {
    const store = {} as Writable<StoreData>;
    createProcessor().process(store);
    expect(store.displayedMargin).toEqual({ longExposure: 0, shortExposure: 0, totalNotional: 0 });
  });

  // Oracle-parity battery: the processor must reproduce the old formula exactly,
  // including floating-point accumulation order, across many shapes.
  const fixtures: Array<{ name: string; positions: DisplayedPosition[] }> = [
    { name: 'empty', positions: [] },
    { name: 'only longs', positions: [pos(L, 1), pos(L, 2), pos(L, 3)] },
    { name: 'only shorts', positions: [pos(S, 5), pos(S, 7)] },
    { name: 'fp-sensitive', positions: [pos(L, 0.1), pos(L, 0.2), pos(S, 0.3), pos(L, 0.1)] },
    { name: 'negatives + zeros', positions: [pos(S, -1234.56), pos(L, 0), pos(S, -0), pos(L, 9.99)] },
    {
      name: 'large deterministic',
      positions: Array.from({ length: 200 }, (_, i) => pos(i % 2 === 0 ? L : S, ((i * 37) % 911) + (i % 7) * 0.137)),
    },
  ];

  for (const { name, positions } of fixtures) {
    it(`matches the reference oracle: ${name}`, () => {
      const store = createStore(positions);
      createProcessor().process(store);
      expect(store.displayedMargin).toEqual(oracle(positions));
    });
  }
});
