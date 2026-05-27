import type { OrderBook } from '../../types';

import { describe, expect, it } from 'vitest';

import {
  applyBinanceDepthUpdate,
  getBufferedBinanceDepthUpdates,
  loadBinanceOrderBookSnapshot,
} from './binance.orderbook';

const createBook = (): OrderBook => ({ bids: [], asks: [] });

describe('binance orderbook sync helpers', () => {
  it('loads a snapshot and applies a first update that overlaps the snapshot update id', () => {
    const book = createBook();
    const state = {};

    loadBinanceOrderBookSnapshot(
      book,
      {
        lastUpdateId: 100,
        bids: [['10', '1']],
        asks: [['11', '2']],
      },
      state,
    );

    const result = applyBinanceDepthUpdate(
      book,
      {
        U: 99,
        u: 101,
        b: [['10', '3']],
        a: [['12', '4']],
      },
      state,
    );

    expect(result).toBe('applied');
    expect(book.bids.map(({ price, amount }) => [price, amount])).toEqual([[10, 3]]);
    expect(book.asks.map(({ price, amount }) => [price, amount])).toEqual([
      [11, 2],
      [12, 4],
    ]);
  });

  it('detects a spot-style sequence gap when first update id skips past the local book', () => {
    const book = createBook();
    const state = {};

    loadBinanceOrderBookSnapshot(
      book,
      {
        lastUpdateId: 100,
        bids: [['10', '1']],
        asks: [['11', '2']],
      },
      state,
    );

    expect(
      applyBinanceDepthUpdate(
        book,
        {
          U: 102,
          u: 103,
          b: [['10', '3']],
          a: [],
        },
        state,
      ),
    ).toBe('gap');
    expect(book.bids.map(({ price, amount }) => [price, amount])).toEqual([[10, 1]]);
  });

  it('detects a futures-style sequence gap when pu does not match the previous final update id', () => {
    const book = createBook();
    const state = {};

    loadBinanceOrderBookSnapshot(
      book,
      {
        lastUpdateId: 100,
        bids: [['10', '1']],
        asks: [['11', '2']],
      },
      state,
    );

    expect(
      applyBinanceDepthUpdate(
        book,
        {
          U: 100,
          u: 101,
          pu: 99,
          b: [['10', '3']],
          a: [],
        },
        state,
        { requirePreviousFinalUpdateId: true },
      ),
    ).toBe('applied');

    expect(
      applyBinanceDepthUpdate(
        book,
        {
          U: 102,
          u: 103,
          pu: 100,
          b: [['10', '4']],
          a: [],
        },
        state,
        { requirePreviousFinalUpdateId: true },
      ),
    ).toBe('gap');
    expect(book.bids.map(({ price, amount }) => [price, amount])).toEqual([[10, 3]]);
  });

  it('ignores stale updates and filters buffered updates using snapshot lastUpdateId', () => {
    const book = createBook();
    const state = {};

    loadBinanceOrderBookSnapshot(
      book,
      {
        lastUpdateId: 100,
        bids: [['10', '1']],
        asks: [['11', '2']],
      },
      state,
    );

    expect(applyBinanceDepthUpdate(book, { U: 98, u: 100, b: [['10', '3']], a: [] }, state)).toBe('ignored');
    expect(book.bids.map(({ price, amount }) => [price, amount])).toEqual([[10, 1]]);
    expect(getBufferedBinanceDepthUpdates([{ u: 99 }, { u: 100 }, { u: 101 }], 100)).toEqual([{ u: 101 }]);
  });
});
