import type { Candle } from '../generated/Candle';
import type { OrderBook } from '../generated/OrderBook';

import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

import { beforeAll, describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { KeyValueCandleCacheBackend } from '../candleCache/backends/KeyValueCandleCacheBackend';
import { OrderSide } from '../types';
import { calcOrderBookTotal, sortOrderBook } from '../utils/orderbook';
import { BookFrames } from './bookFrames';
import { readCandleFrame } from './candleFrames';
import { marketDataFrameApplier } from './marketDataFrameApplier.fixture';
import { DefaultStore } from './store.base';

const FrameApplier = marketDataFrameApplier();

let fixtures: Record<string, number[]>;
const symbols = ['ZERO/USD'];
const ids = ['zero'];
const frame = (name: string) => new Frame(Uint8Array.from(fixtures[name]).buffer);

beforeAll(() => {
  const root = resolve(__dirname, '../../../..');
  fixtures = JSON.parse(
    execFileSync(
      'cargo',
      [
        'run',
        '-j2',
        '--locked',
        '--quiet',
        '--manifest-path',
        `${root}/sdk-rs/Cargo.toml`,
        '-p',
        'safe-cex-engine',
        '--example',
        'market_data_fixture',
      ],
      { cwd: root, encoding: 'utf8' },
    ),
  );
}, 120_000);

describe('Rust market data frames', () => {
  it('decodes native snapshots with mutable zero-price levels and immutable retained baselines', () => {
    const reader = new BookFrames();
    const snapshot = frame('snapshot');
    const book = reader.read(snapshot, symbols, ids)[0].orderbook;
    expect(book).toEqual({
      bids: [
        { price: 2, amount: 1, total: 1 },
        { price: 0, amount: 2, total: 3 },
      ],
      asks: [{ price: 3, amount: 4, total: 4 }],
    });
    const retained = structuredClone(book);
    structuredClone(snapshot.buffer, { transfer: [snapshot.buffer] });
    expect(snapshot.buffer.byteLength).toBe(0);
    book.bids[0].amount = 999;
    book.asks = [];
    const delta = reader.read(frame('delta'), symbols, ids)[0].orderbook;
    expect(delta).toEqual({
      bids: [
        { price: 1, amount: 5, total: 5 },
        { price: 0, amount: 2, total: 7 },
      ],
      asks: [{ price: 0, amount: 3, total: 3 }],
    });
    expect(retained.bids[0].amount).toBe(1);
    expect(reader.read(frame('empty'), symbols, ids)[0].orderbook).toEqual({ bids: [], asks: [] });
  });

  it('requires a fresh snapshot after market identity changes or explicit clearing', () => {
    const reader = new BookFrames();
    expect(() => reader.read(frame('delta'), symbols, ids)).toThrow(/snapshot required/);
    reader.read(frame('snapshot'), symbols, ids);
    reader.read(frame('idle'), symbols, ['replacement']);
    expect(() => reader.read(frame('delta'), symbols, ['replacement'])).toThrow(/snapshot required/);
    reader.read(frame('snapshot'), symbols, ids);
    reader.clear();
    expect(() => reader.read(frame('delta'), symbols, ids)).toThrow(/snapshot required/);
    expect(() => reader.read(frame('snapshot'), [], [])).toThrow(/unknown market/);
  });

  it('rejects corrupt snapshots without silently sorting, coercing or advancing the baseline', () => {
    const reader = new BookFrames();
    reader.read(frame('snapshot'), symbols, ids);
    for (const name of ['invalid', 'unordered', 'negative', 'invalidTotal', 'invalidDelta']) {
      expect(() => reader.read(frame(name), symbols, ids)).toThrow(/invalid|sorted/);
    }
    expect(reader.read(frame('delta'), symbols, ids)[0].orderbook.bids.at(-1)?.total).toBe(7);
  });

  it('caches a frame once while consumers can use the generated mutable book contract', () => {
    const reader = new BookFrames();
    const snapshot = frame('snapshot');
    const result = reader.read(snapshot, symbols, ids);
    expect(reader.read(snapshot, symbols, ids)).toBe(result);
    const book: OrderBook = structuredClone(result[0].orderbook);
    book.bids.reverse();
    sortOrderBook(book);
    calcOrderBookTotal(book);
    expect(book).toEqual(result[0].orderbook);
  });

  it('matches the DefaultStore frame path and serialization without inventing stored books or candles', () => {
    const snapshot = frame('snapshot');
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(snapshot);
    const serialized = JSON.parse(JSON.stringify(store.serialize()));
    expect(applier.books(snapshot)).toEqual([
      {
        symbol: 'ZERO/USD',
        orderbook: {
          bids: [
            { price: 2, amount: 1, total: 1 },
            { price: 0, amount: 2, total: 3 },
          ],
          asks: [{ price: 3, amount: 4, total: 4 }],
        },
      },
    ]);
    expect(applier.candles(snapshot)).toEqual([
      {
        symbol: 'ZERO/USD',
        interval: '1m',
        candle: {
          timestamp: 1_700_000_040_000,
          open: 0,
          high: 2,
          low: 0,
          close: 1,
          volume: 0,
        },
      },
    ]);
    expect(serialized.markets[0].symbol).toBe('ZERO/USD');
    expect(serialized).not.toHaveProperty('books');
    expect(serialized).not.toHaveProperty('candles');
  });

  it.each(['unordered', 'invalidTotal'])(
    'integrated FrameApplier refuses %s book batches and discards their baselines',
    (name) => {
      const store = new DefaultStore();
      const applier = new FrameApplier(store);
      applier.apply(frame('snapshot'));
      const invalid = frame(name);
      expect(() => applier.apply(invalid)).toThrow();
      expect(() => applier.books(invalid)).toThrow();
      expect(() => applier.apply(frame('delta'))).toThrow();
      applier.apply(frame('snapshot'));
      expect(applier.books(frame('delta'))[0].orderbook.bids[0].price).toBe(1);
      expect(JSON.parse(JSON.stringify(store.serialize())).markets[0].symbol).toBe('ZERO/USD');
    },
  );

  it.each(['unknownCandle', 'invalidCandle'])(
    'integrated FrameApplier refuses %s candle batches without emitting repaired values',
    (name) => {
      const store = new DefaultStore();
      const applier = new FrameApplier(store);
      applier.apply(frame('snapshot'));
      const invalid = frame(name);
      applier.apply(invalid);
      expect(() => applier.candles(invalid)).toThrow();
      expect(JSON.parse(JSON.stringify(store.serialize()))).not.toHaveProperty('candles');
      expect(applier.candles(frame('snapshot'))[0].candle.volume).toBe(0);
    },
  );

  it('preserves candle milliseconds, zero prices, zero volume and absence of lastSide', () => {
    const row = readCandleFrame(frame('snapshot'), symbols)[0];
    expect(row).toEqual({
      symbol: 'ZERO/USD',
      interval: '1m',
      candle: {
        timestamp: 1_700_000_040_000,
        open: 0,
        high: 2,
        low: 0,
        close: 1,
        volume: 0,
      },
    });
    expect(row.candle).not.toHaveProperty('lastSide');
    expect(readCandleFrame(frame('idle'), symbols)).toEqual([]);
    expect(() => readCandleFrame(frame('unknownCandle'), symbols)).toThrow(/identity/);
    expect(() => readCandleFrame(frame('invalidCandle'), symbols)).toThrow(/Invalid/);
  });

  it('round-trips generated candles through the real persistent cache with omitted and present enums', async () => {
    const values = new Map<string, string>();
    const backend = new KeyValueCandleCacheBackend({
      get: async (key) => values.get(key) ?? null,
      set: async (key, value) => {
        values.set(key, value);
      },
      delete: async (key) => {
        values.delete(key);
      },
      listKeys: async (prefix) => [...values.keys()].filter((key) => key.startsWith(prefix)),
    });
    expect(await backend.loadChunks('missing', [0], { chunkSize: 100, intervalSeconds: 60 })).toEqual(new Map());
    const candle = readCandleFrame(frame('snapshot'), symbols)[0].candle;
    const intervalSeconds = 60;
    const chunkSize = 100;
    const index = candle.timestamp / 1000 / intervalSeconds;
    const chunk = Math.floor(index / chunkSize);
    const offset = index % chunkSize;
    const sparse = new Array<Candle | undefined>(chunkSize);
    sparse[offset] = candle;
    sparse[offset + 1] = { ...candle, timestamp: candle.timestamp + 60_000, lastSide: OrderSide.Sell };
    await backend.persistChunks('zero', new Map([[chunk, sparse]]));
    const loaded = (await backend.loadChunks('zero', [chunk], { chunkSize, intervalSeconds })).get(chunk);
    if (!loaded) throw new Error('Expected the persisted candle chunk');
    expect(loaded[offset]).toEqual(candle);
    expect(loaded[offset]).not.toHaveProperty('lastSide');
    expect(loaded[offset + 1]?.lastSide).toBe(OrderSide.Sell);
    expect(loaded[offset + 2]).toBeUndefined();
  });
});
