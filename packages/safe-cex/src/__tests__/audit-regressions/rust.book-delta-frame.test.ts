import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { describe, expect, it } from 'vitest';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

type Book = { market: number; bids: number[]; asks: number[] };
const frame = ({ markets, books = [], deltas = [] }: { markets?: Array<[string, string]>; books?: Book[]; deltas?: Book[] } = {}): Frame => ({
  markets: () => markets && {
    count: markets.length,
    id: (i: number) => markets[i][0], symbol: (i: number) => markets[i][1],
    base: () => 'BTC', quote: () => 'USDT',
    ...Object.fromEntries(['flags', 'precisionAmount', 'precisionPrice', 'minAmount', 'maxAmount', 'minLeverage', 'maxLeverage', 'minPrice', 'contractSize', 'takerFee', 'makerFee'].map((key) => [key, Float64Array.from(markets, () => 1)])),
  },
  tickers: () => undefined, tickerDeltas: () => undefined, orders: () => undefined,
  positions: () => undefined, balance: () => undefined, status: () => undefined,
  settings: () => undefined,
  books: function* () { for (const book of books) yield { market: book.market, bids: Float64Array.from(book.bids), asks: Float64Array.from(book.asks) }; },
  bookDeltas: function* () { for (const book of deltas) yield { market: book.market, bids: Float64Array.from(book.bids), asks: Float64Array.from(book.asks) }; },
}) as unknown as Frame;
const initial = () => frame({ markets: [['btc-native', 'BTCUSDT'], ['eth-native', 'ETHUSDT']], books: [
  { market: 0, bids: [99, 2, 2, 98, 3, 5], asks: [101, 4, 4, 102, 5, 9] },
  { market: 1, bids: [49, 6, 6], asks: [51, 7, 7] },
] });
const levels = (rows: number[][]) => rows.map(([price, amount, total]) => ({ price, amount, total }));

describe('Rust BOOK_DELTA reconstruction', () => {
  it('updates and deletes levels, inserts best prices, sorts and recomputes cumulative totals', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    applier.apply(first);
    const retained = applier.books(first);
    const delta = frame({ deltas: [{ market: 0, bids: [99, 0, 98, 8, 100, 1], asks: [101, 0, 102, 2, 100.5, 3] }] });
    applier.apply(delta);
    expect(applier.books(delta)).toEqual([{ symbol: 'BTCUSDT', orderbook: {
      bids: levels([[100, 1, 1], [98, 8, 9]]), asks: levels([[100.5, 3, 3], [102, 2, 5]]),
    } }]);
    expect(retained[0].orderbook.bids).toEqual(levels([[99, 2, 2], [98, 3, 5]]));
    expect(applier.books(first)).toBe(retained);
    expect(applier.books(delta)).toBe(applier.books(delta));
    // Rereading an old frame must not roll the live baseline back.
    applier.apply(delta);
    const next = frame({ deltas: [{ market: 0, bids: [98, 0], asks: [] }] });
    applier.apply(next);
    expect(applier.books(next)[0].orderbook.bids).toEqual(levels([[100, 1, 1]]));
  });

  it('advances unobserved frames and isolates emitted objects from internal baselines', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    applier.apply(first);
    // An unread initial snapshot still establishes the next delta baseline.
    applier.books(first)[0].orderbook.bids[0].amount = 1000;
    applier.apply(frame({ deltas: [{ market: 0, bids: [99, 4], asks: [] }] }));
    const next = frame({ deltas: [{ market: 0, bids: [97, 1], asks: [] }] });
    applier.apply(next);
    expect(applier.books(next)[0].orderbook.bids).toEqual(levels([[99, 4, 4], [98, 3, 7], [97, 1, 8]]));
  });

  it('preserves stable identities across market reorder and rejects replaced or removed baselines', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    applier.apply(first);
    applier.books(first);
    const reorder = frame({ markets: [['eth-native', 'ETHUSDT'], ['btc-native', 'BTCUSDT']], deltas: [
      { market: 0, bids: [49, 8], asks: [] }, { market: 1, bids: [], asks: [101, 6] },
    ] });
    applier.apply(reorder);
    expect(applier.books(reorder).map((book) => [book.symbol, book.orderbook.bids[0].amount])).toEqual([['ETHUSDT', 8], ['BTCUSDT', 2]]);
    expect(() => applier.apply(frame({ markets: [['other-id', 'BTCUSDT']], deltas: [{ market: 0, bids: [99, 1], asks: [] }] }))).toThrow(/snapshot required/);
    applier.apply(frame({ markets: [['other-id', 'BTCUSDT']] }));
    expect(() => applier.apply(frame({ deltas: [{ market: 0, bids: [99, 1], asks: [] }] }))).toThrow(/snapshot required/);
    applier.apply(frame({ markets: [] }));
    applier.apply(frame({ markets: [['btc-native', 'BTCUSDT']] }));
    expect(() => applier.apply(frame({ deltas: [{ market: 0, bids: [99, 1], asks: [] }] }))).toThrow(/snapshot required/);
  });

  it('full resets clear old prices, permit empty baselines and precede same-frame deltas', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    applier.apply(first);
    applier.books(first);
    const reset = frame({ books: [{ market: 0, bids: [], asks: [] }], deltas: [{ market: 0, bids: [100, 2], asks: [] }] });
    applier.apply(reset);
    expect(applier.books(reset)).toEqual([{ symbol: 'BTCUSDT', orderbook: { bids: levels([[100, 2, 2]]), asks: [] } }]);
    const empty = frame({ books: [{ market: 0, bids: [], asks: [] }] });
    applier.apply(empty);
    expect(applier.books(empty)[0].orderbook).toEqual({ bids: [], asks: [] });
  });

  it('retains valid store sections while rejecting malformed multi-book changes atomically', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    expect(() => applier.apply(frame({ markets: [['btc-native', 'BTCUSDT']], deltas: [{ market: 0, bids: [99, 1], asks: [] }] }))).toThrow(/snapshot required/);
    expect(store.markets).toHaveLength(1);
    const first = initial();
    applier.apply(first);
    applier.books(first);
    for (const invalid of [NaN, Infinity, -1]) {
      applier.apply(initial());
      expect(() => applier.apply(frame({ deltas: [
        { market: 0, bids: [99, 10], asks: [] }, { market: 1, bids: [49, invalid], asks: [] },
      ] }))).toThrow(/snapshot required/);
    }
    expect(() => applier.apply(frame({ deltas: [{ market: 200, bids: [], asks: [] }] }))).toThrow(/snapshot required/);
    const good = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    expect(() => applier.apply(good)).toThrow(/snapshot required/);
    applier.apply(initial());
    const resumed = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    applier.apply(resumed);
    expect(applier.books(good)).toEqual([]);
    expect(applier.books(resumed)[0].orderbook.bids).toEqual(levels([[99, 2, 2], [98, 4, 6]]));
  });

  it('keeps rejected book batches invalid until a recovery snapshot arrives while preserving account sections and fills', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    const first = initial();
    applier.apply(first);
    const retained = applier.books(first);
    const broken = frame({ deltas: [
      { market: 0, bids: [99, 10], asks: [] },
      { market: 1, bids: [49, NaN], asks: [] },
    ] });
    broken.balance = () => ({ used: 1, free: 6, total: 7, upnl: 0 });
    broken.fills = () => ({ count: 1, market: Uint32Array.of(0), side: Uint32Array.of(0),
      rows: Float64Array.of(99, 0.25), orderId: () => 'tracked-order' });
    expect(() => applier.apply(broken)).toThrow(/snapshot required/);
    expect(applier.books(broken)).toEqual([]);
    expect(store.balance.total).toBe(7);
    expect(applier.fills(broken)[0]).toMatchObject({ orderId: 'tracked-order', symbol: 'BTCUSDT', amount: 0.25 });
    expect(retained[0].orderbook.bids[0].amount).toBe(2);

    // A failed recovery request leaves no full snapshot to restore the dropped deltas.
    const next = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    next.balance = () => ({ used: 1, free: 7, total: 8, upnl: 0 });
    expect(() => applier.apply(next)).toThrow(/snapshot required/);
    expect(applier.books(next)).toEqual([]);
    expect(store.balance.total).toBe(8);

    applier.apply(frame({ books: [{ market: 0, bids: [99, 10, 10, 98, 3, 13], asks: [] }] }));
    const resumed = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    applier.apply(resumed);
    expect(applier.books(resumed)[0].orderbook.bids).toEqual(levels([[99, 10, 10], [98, 4, 14]]));
  });

  it('supports reading a self-contained snapshot directly before apply', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    expect(applier.books(first)).toHaveLength(2);
    applier.apply(first);
    const next = frame({ deltas: [{ market: 0, bids: [99, 1], asks: [] }] });
    applier.apply(next);
    expect(applier.books(next)[0].orderbook.bids).toEqual(levels([[99, 1, 1], [98, 3, 4]]));
  });
  it('preserves the baseline after its source buffer is transferred', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    const bids = Float64Array.of(99, 2, 2, 98, 3, 5);
    first.books = function* () { yield { market: 0, bids, asks: new Float64Array() }; };
    applier.apply(first);
    const old = applier.books(first);
    structuredClone(bids.buffer, { transfer: [bids.buffer] });
    expect(bids.byteLength).toBe(0);
    const next = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    applier.apply(next);
    expect(applier.books(next)[0].orderbook.bids).toEqual(levels([[99, 2, 2], [98, 4, 6]]));
    expect(old[0].orderbook.bids).toEqual(levels([[99, 2, 2], [98, 3, 5]]));
  });

  it('explicitly skips forwarded browser books before transferring their frames', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = initial();
    first.books = function* () { throw new Error('books must stay lazy'); };
    expect(() => applier.apply(first, false)).not.toThrow();
    expect(() => applier.books(first)).toThrow('books must stay lazy');
  });

  it('advances unread snapshots and deltas before the first book read', () => {
    const applier = new FrameApplier(new DefaultStore());
    applier.apply(initial());
    applier.apply(frame({ deltas: [{ market: 0, bids: [99, 8], asks: [] }] }));
    const next = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    applier.apply(next);
    expect(applier.books(next)[0].orderbook.bids).toEqual(levels([[99, 8, 8], [98, 4, 12]]));
  });

  it('invalidates skipped book baselines and requires a new snapshot before deltas resume', () => {
    const applier = new FrameApplier(new DefaultStore());
    applier.apply(initial());
    applier.apply(frame({ deltas: [{ market: 0, bids: [99, 8], asks: [] }] }), false);
    expect(() => applier.apply(frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] }))).toThrow(/snapshot required/);
    applier.apply(initial());
    const next = frame({ deltas: [{ market: 0, bids: [98, 4], asks: [] }] });
    applier.apply(next);
    expect(applier.books(next)[0].orderbook.bids).toEqual(levels([[99, 2, 2], [98, 4, 6]]));
  });

  it('retains a legitimate zero-price level through a full baseline, update and deletion', () => {
    const applier = new FrameApplier(new DefaultStore());
    const first = frame({ markets: [['outcome-native', 'OUTCOME']], books: [
      { market: 0, bids: [0.5, 3, 3, 0, 2, 5], asks: [] },
    ] });
    applier.apply(first);
    expect(applier.books(first)[0].orderbook.bids).toEqual(levels([[0.5, 3, 3], [0, 2, 5]]));
    const update = frame({ deltas: [{ market: 0, bids: [0, 5], asks: [] }] });
    applier.apply(update);
    expect(applier.books(update)[0].orderbook.bids).toEqual(levels([[0.5, 3, 3], [0, 5, 8]]));
    const deletion = frame({ deltas: [{ market: 0, bids: [0, 0], asks: [] }] });
    applier.apply(deletion);
    expect(applier.books(deletion)[0].orderbook.bids).toEqual(levels([[0.5, 3, 3]]));
    expect(applier.books(first)[0].orderbook.bids).toEqual(levels([[0.5, 3, 3], [0, 2, 5]]));
  });

});
