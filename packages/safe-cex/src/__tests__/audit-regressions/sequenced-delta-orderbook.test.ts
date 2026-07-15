import type { OrderBook } from '../../types';
import type { SequencedBookDelta } from '../../utils/sequencedDeltaOrderBook';

import { describe, expect, it } from 'vitest';

import { SequencedDeltaOrderBook } from '../../utils/sequencedDeltaOrderBook';

const createBook = () => {
  const orderBook: OrderBook = { bids: [], asks: [] };
  return { orderBook, sequenced: new SequencedDeltaOrderBook(orderBook) };
};

const delta = (overrides: Partial<SequencedBookDelta> = {}): SequencedBookDelta => ({
  sequence: '101',
  previousSequence: '100',
  bids: [],
  asks: [],
  ...overrides,
});

describe('sequenced delta orderbook foundation', () => {
  it('initializes only from an explicit snapshot and sorts its levels', () => {
    const { orderBook, sequenced } = createBook();

    expect(
      sequenced.applySnapshot({
        sequence: '900719925474099312345',
        bids: [
          [99, 2],
          [101, 1],
          [100, 0],
        ],
        asks: [
          [103, 3],
          [102, 4],
        ],
      }),
    ).toEqual({ status: 'applied', frame: 'snapshot', sequence: '900719925474099312345' });
    expect(orderBook).toEqual({
      bids: [
        { price: 101, amount: 1, total: 0 },
        { price: 99, amount: 2, total: 0 },
      ],
      asks: [
        { price: 102, amount: 4, total: 0 },
        { price: 103, amount: 3, total: 0 },
      ],
    });
    expect(sequenced.currentSequence).toBe('900719925474099312345');
    expect(sequenced.isReady).toBe(true);
  });

  it('emits one recovery signal for deltas received before a snapshot', () => {
    const { orderBook, sequenced } = createBook();

    expect(sequenced.applyDelta(delta())).toEqual({ status: 'recover', reason: 'missing-snapshot' });
    expect(sequenced.applyDelta(delta({ sequence: '102', previousSequence: '101' }))).toEqual({
      status: 'ignored',
      reason: 'awaiting-snapshot',
    });
    expect(orderBook).toEqual({ bids: [], asks: [] });
    expect(sequenced.isRecovering).toBe(true);
  });

  it('applies contiguous upserts and deletes while preserving sort order', () => {
    const { orderBook, sequenced } = createBook();
    sequenced.applySnapshot({
      sequence: '100',
      bids: [
        [100, 2],
        [99, 1],
      ],
      asks: [
        [101, 3],
        [102, 4],
      ],
    });

    expect(
      sequenced.applyDelta(
        delta({
          sequence: '105',
          previousSequence: '100',
          bids: [
            [100, 0],
            [100.5, 5],
          ],
          asks: [
            [102, 6],
            [100.75, 7],
          ],
        }),
      ),
    ).toEqual({ status: 'applied', frame: 'delta', sequence: '105' });
    expect(orderBook).toEqual({
      bids: [
        { price: 100.5, amount: 5, total: 0 },
        { price: 99, amount: 1, total: 0 },
      ],
      asks: [
        { price: 100.75, amount: 7, total: 0 },
        { price: 101, amount: 3, total: 0 },
        { price: 102, amount: 6, total: 0 },
      ],
    });
  });

  it('ignores duplicate and stale deltas without mutating the live book', () => {
    const { orderBook, sequenced } = createBook();
    sequenced.applySnapshot({ sequence: 100, bids: [[100, 1]], asks: [[101, 1]] });

    expect(sequenced.applyDelta(delta({ sequence: 100, previousSequence: 99, bids: [[100, 9]] }))).toEqual({
      status: 'ignored',
      reason: 'duplicate',
      sequence: '100',
    });
    expect(sequenced.applyDelta(delta({ sequence: 98, previousSequence: 97, asks: [[101, 9]] }))).toEqual({
      status: 'ignored',
      reason: 'stale',
      sequence: '98',
    });
    expect(orderBook).toEqual({
      bids: [{ price: 100, amount: 1, total: 0 }],
      asks: [{ price: 101, amount: 1, total: 0 }],
    });
    expect(sequenced.currentSequence).toBe('100');
  });

  it('clears a discontinuous book and suppresses deltas until a fresh snapshot', () => {
    const { orderBook, sequenced } = createBook();
    sequenced.applySnapshot({ sequence: 100, bids: [[100, 1]], asks: [[101, 1]] });

    expect(sequenced.applyDelta(delta({ sequence: 102, previousSequence: 99 }))).toEqual({
      status: 'recover',
      reason: 'sequence-gap',
    });
    expect(orderBook).toEqual({ bids: [], asks: [] });
    expect(sequenced.applyDelta(delta({ sequence: 103, previousSequence: 102 }))).toEqual({
      status: 'ignored',
      reason: 'awaiting-snapshot',
    });

    expect(sequenced.applySnapshot({ sequence: 200, bids: [[200, 2]], asks: [[201, 3]] })).toEqual({
      status: 'applied',
      frame: 'snapshot',
      sequence: '200',
    });
    expect(sequenced.applyDelta(delta({ sequence: 202, previousSequence: 200, bids: [[199, 4]] }))).toEqual({
      status: 'applied',
      frame: 'delta',
      sequence: '202',
    });
  });

  it('rejects unsafe sequences and malformed levels atomically', () => {
    const { orderBook, sequenced } = createBook();
    sequenced.applySnapshot({ sequence: 100, bids: [[100, 1]], asks: [[101, 1]] });

    expect(
      sequenced.applyDelta(
        delta({
          sequence: Number.MAX_SAFE_INTEGER + 1,
          previousSequence: 100,
          bids: [[99, Number.NaN]],
        }),
      ),
    ).toEqual({ status: 'recover', reason: 'invalid-sequence' });
    expect(orderBook).toEqual({ bids: [], asks: [] });

    sequenced.applySnapshot({ sequence: 200, bids: [[200, 2]], asks: [[201, 3]] });
    expect(sequenced.applyDelta(delta({ sequence: 201, previousSequence: 200, asks: [[0, 4]] }))).toEqual({
      status: 'recover',
      reason: 'invalid-level',
    });
    expect(orderBook).toEqual({ bids: [], asks: [] });

    sequenced.applySnapshot({ sequence: 300, bids: [[300, 2]], asks: [[301, 3]] });
    expect(
      sequenced.applyDelta(
        delta({ sequence: 301, previousSequence: 300, bids: [undefined as unknown as readonly [number, number]] }),
      ),
    ).toEqual({ status: 'recover', reason: 'invalid-level' });
    expect(orderBook).toEqual({ bids: [], asks: [] });
  });

  it('reset removes sequence state and clears the book', () => {
    const { orderBook, sequenced } = createBook();
    sequenced.applySnapshot({ sequence: 100, bids: [[100, 1]], asks: [[101, 1]] });

    sequenced.reset();

    expect(orderBook).toEqual({ bids: [], asks: [] });
    expect(sequenced.currentSequence).toBeUndefined();
    expect(sequenced.isReady).toBe(false);
    expect(sequenced.isRecovering).toBe(false);
  });
});
