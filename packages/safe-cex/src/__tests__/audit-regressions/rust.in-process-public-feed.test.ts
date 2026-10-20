// @vitest-environment node
import type { Candle, Market, OHLCVOptions, OrderBook, PublicDataSource, StoreData, Ticker } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { inProcessPortPair, RustPublicFeedClient, serveInProcessPublicData } from '../../exchanges/rust/publicFeed';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const market = (symbol: string) =>
  ({ id: symbol, symbol, contractSize: 1, isSpot: false, isPerpetual: true }) as Market;
const ticker = (symbol: string, last: number) =>
  ({ id: symbol, symbol, bid: last - 1, ask: last + 1, last, mark: last, index: last }) as Ticker;

/** exchange-server's shared source: a live view over whichever public exchange is current. */
function restartableSource() {
  type Exchange = {
    markets: Market[];
    tickers: Ticker[];
    loaded: StoreData['loaded'];
    listeners: Set<() => void>;
    books: Map<string, (book: OrderBook) => void>;
    candles: Map<string, (candle: Candle) => void>;
  };
  const create = (): Exchange => ({
    markets: [],
    tickers: [],
    loaded: { markets: false, tickers: false, balance: false, orders: false, positions: false },
    listeners: new Set(),
    books: new Map(),
    candles: new Map(),
  });
  let current = create();
  const storeListeners = new Set<() => void>();
  const source: PublicDataSource = {
    store: {
      subscribe: (callback) => {
        const listener = () => callback({} as StoreData);
        storeListeners.add(listener);
        return () => storeListeners.delete(listener);
      },
      get tickers() {
        return current.tickers;
      },
      get markets() {
        return current.markets;
      },
      get loaded() {
        return current.loaded;
      },
    },
    listenOrderBook: (symbol, callback) => {
      const books = current.books;
      books.set(symbol, callback);
      return () => books.delete(symbol);
    },
    listenOHLCV: (opts: OHLCVOptions, callback) => {
      const candles = current.candles;
      candles.set(`${opts.symbol}:${opts.interval}`, callback);
      return () => candles.delete(`${opts.symbol}:${opts.interval}`);
    },
  };
  const notify = () => storeListeners.forEach((listener) => listener());
  return {
    source,
    get current() {
      return current;
    },
    load(markets: Market[], tickers: Ticker[]) {
      current.markets = markets;
      current.tickers = tickers;
      current.loaded = { ...current.loaded, markets: true, tickers: true };
      notify();
    },
    restart() {
      current = create();
      notify();
    },
    notify,
    storeListenerCount: () => storeListeners.size,
  };
}

function account(source: PublicDataSource) {
  const [port, sourcePort] = inProcessPortPair();
  const stop = serveInProcessPublicData(source, sourcePort);
  const updates: Array<Record<string, unknown>> = [];
  const failures: Error[] = [];
  const client = new RustPublicFeedClient(
    port,
    'identity is not checked in-process',
    async (update) => updates.push(update),
    (error) => failures.push(error),
    { handshakeTimeoutMs: 30_000, inProcess: true },
  );
  return { client, updates, failures, stop, connected: client.connect() };
}

describe('Rust in-process shared public data (exchange-server externalPublicData)', () => {
  afterEach(() => vi.useRealTimers());

  it('waits for the source to load, then serves two accounts and survives a source restart', async () => {
    vi.useFakeTimers();
    const venue = restartableSource();
    const first = account(venue.source);
    const second = account(venue.source);
    let settled = false;
    void Promise.all([first.connected, second.connected]).then(() => (settled = true));
    await vi.advanceTimersByTimeAsync(10_000);
    expect(settled).toBe(false);
    // One store subscription per source, however many accounts attach.
    expect(venue.storeListenerCount()).toBe(1);

    venue.load([market('BTCUSDT'), market('ETHUSDT')], [ticker('BTCUSDT', 100), ticker('ETHUSDT', 10)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(settled).toBe(true);
    for (const side of [first, second]) {
      expect(side.client.isReady).toBe(true);
      side.client.validateMarkets([market('BTCUSDT')]);
      expect(side.updates.at(-1)).toMatchObject({ tickers: [{ symbol: 'BTCUSDT', last: 100 }, { symbol: 'ETHUSDT' }] });
    }

    // Notifies coalesce into one delta per interval, carrying only changed tickers.
    (venue.current.tickers[0] as { last: number }).last = 101;
    venue.notify();
    venue.notify();
    const before = first.updates.length;
    await vi.advanceTimersByTimeAsync(250);
    expect(first.updates.length).toBe(before + 1);
    expect(first.updates.at(-1)).toEqual({ op: 'publicFeedUpdate', tickers: [expect.objectContaining({ last: 101 })] });
    venue.notify();
    await vi.advanceTimersByTimeAsync(250);
    expect(first.updates.length).toBe(before + 1);

    first.client.send({ op: 'book', symbol: 'BTCUSDT', subscribe: true });
    second.client.send({ op: 'candle', symbol: 'ETHUSDT', interval: '5m', subscribe: true });
    await vi.advanceTimersByTimeAsync(0);
    venue.current.books.get('BTCUSDT')!({ bids: [{ price: 99, amount: 2, total: 2 }], asks: [] });
    venue.current.candles.get('ETHUSDT:5m')!({ timestamp: 1, open: 1, high: 2, low: 1, close: 2, volume: 3 });
    await vi.advanceTimersByTimeAsync(0);
    expect(first.updates.at(-1)).toEqual({
      op: 'publicFeedUpdate',
      books: [{ symbol: 'BTCUSDT', bids: [[99, 2]], asks: [] }],
    });
    expect(second.updates.at(-1)).toMatchObject({ candles: [{ symbol: 'ETHUSDT', interval: '5m', close: 2 }] });

    // The server disposes and re-inits its public exchange; the store is empty meanwhile.
    venue.restart();
    first.client.send({ op: 'book', symbol: 'ETHUSDT', subscribe: true });
    await vi.advanceTimersByTimeAsync(20_000);
    venue.load([market('BTCUSDT'), market('ETHUSDT')], [ticker('BTCUSDT', 105), ticker('ETHUSDT', 11)]);
    await vi.advanceTimersByTimeAsync(250);
    for (const side of [first, second]) {
      expect(side.failures).toEqual([]);
      expect(side.updates.at(-1)).toMatchObject({ tickers: [{ symbol: 'BTCUSDT', last: 105 }, { symbol: 'ETHUSDT' }] });
    }
    venue.current.books.get('ETHUSDT')!({ bids: [], asks: [{ price: 12, amount: 1, total: 1 }] });
    await vi.advanceTimersByTimeAsync(0);
    expect(first.updates.at(-1)).toMatchObject({ books: [{ symbol: 'ETHUSDT', asks: [[12, 1]] }] });

    first.client.dispose();
    second.client.dispose();
    await vi.advanceTimersByTimeAsync(0);
    expect(venue.storeListenerCount()).toBe(0);
  });

  it('lets private startup continue when the source never loads, and accepts it later', async () => {
    vi.useFakeTimers();
    const venue = restartableSource();
    const side = account(venue.source);
    let settled = false;
    void side.connected.then(() => (settled = true));
    await vi.advanceTimersByTimeAsync(30_000);
    expect(settled).toBe(true);
    expect(side.client.isReady).toBe(false);
    expect(side.failures).toEqual([]);
    side.client.send({ op: 'book', symbol: 'BTCUSDT', subscribe: true });
    venue.load([market('BTCUSDT')], [ticker('BTCUSDT', 100)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(side.client.isReady).toBe(true);
    expect(side.updates.at(-1)).toMatchObject({ tickers: [{ symbol: 'BTCUSDT', last: 100 }] });
    expect(venue.current.books.has('BTCUSDT')).toBe(true);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(side.failures).toEqual([]);
    side.client.dispose();
  });

  it('tolerates account markets the source has not listed yet, and serves them once it does', async () => {
    const venue = restartableSource();
    venue.load([market('BTCUSDT')], [ticker('BTCUSDT', 100)]);
    const side = account(venue.source);
    await side.connected;
    side.client.validateMarkets([market('BTCUSDT'), market('SOLUSDT')]);
    expect(() => side.client.validateMarkets([{ ...market('BTCUSDT'), contractSize: 2 }])).toThrow(
      'catalog/product mismatch',
    );
    side.client.send({ op: 'book', symbol: 'SOLUSDT', subscribe: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(venue.current.books.has('SOLUSDT')).toBe(false);
    venue.load([market('BTCUSDT'), market('SOLUSDT')], [ticker('BTCUSDT', 100), ticker('SOLUSDT', 5)]);
    await vi.waitFor(() => expect(venue.current.books.has('SOLUSDT')).toBe(true));
    expect(side.updates.at(-1)).toMatchObject({ tickers: [{ symbol: 'BTCUSDT' }, { symbol: 'SOLUSDT', last: 5 }] });
    expect(side.failures).toEqual([]);
    side.client.dispose();
  });

  it('expires a ready account that goes silent without disposing', async () => {
    vi.useFakeTimers();
    const venue = restartableSource();
    venue.load([market('BTCUSDT')], [ticker('BTCUSDT', 100)]);
    const [port, sourcePort] = inProcessPortPair();
    serveInProcessPublicData(venue.source, sourcePort);
    port.postMessage({ op: 'hello', identity: '' });
    await vi.advanceTimersByTimeAsync(0);
    expect(venue.storeListenerCount()).toBe(1);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(venue.storeListenerCount()).toBe(0);
  });

  it('keeps testnet accounts off the venue-keyed (mainnet) source', () => {
    const { source } = restartableSource();
    const mode = (testnet: boolean) =>
      new RustEngineExchange(
        'bybit',
        { key: 'k', secret: 's', testnet, privateOnly: true, externalPublicData: { type: 'in-process', source } },
        new DefaultStore(),
      ).publicFeedMode;
    expect(mode(false)).toBe('shared');
    expect(mode(true)).toBe('direct');
  });
});
