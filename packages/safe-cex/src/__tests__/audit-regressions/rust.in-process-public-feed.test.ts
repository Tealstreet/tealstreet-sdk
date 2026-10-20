// @vitest-environment node
import type { Candle, Market, OHLCVOptions, OrderBook, PublicDataSource, StoreData, Ticker } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { inProcessPortPair, RustPublicFeedClient, serveInProcessPublicData } from '../../exchanges/rust/publicFeed';

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
    30_000,
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

    venue.load([market('BTCUSDT'), market('ETHUSDT')], [ticker('BTCUSDT', 100), ticker('ETHUSDT', 10)]);
    await vi.advanceTimersByTimeAsync(0);
    expect(settled).toBe(true);
    for (const side of [first, second]) {
      side.client.validateMarkets([market('BTCUSDT')]);
      expect(side.updates.at(-1)).toMatchObject({ tickers: [{ symbol: 'BTCUSDT', last: 100 }, { symbol: 'ETHUSDT' }] });
    }

    // Ticker notifies coalesce into one seed per interval.
    (venue.current.tickers[0] as { last: number }).last = 101;
    venue.notify();
    venue.notify();
    const before = first.updates.length;
    await vi.advanceTimersByTimeAsync(250);
    expect(first.updates.length).toBe(before + 1);
    expect(first.updates.at(-1)).toMatchObject({ tickers: [{ symbol: 'BTCUSDT', last: 101 }, { symbol: 'ETHUSDT' }] });

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

    first.client.dispose();
    second.client.dispose();
    await vi.advanceTimersByTimeAsync(0);
    expect(venue.storeListenerCount()).toBe(0);
  });

  it('re-announces a changed catalog instead of failing the account', async () => {
    const venue = restartableSource();
    venue.load([market('BTCUSDT')], [ticker('BTCUSDT', 100)]);
    const side = account(venue.source);
    await side.connected;
    expect(() => side.client.validateMarkets([market('SOLUSDT')])).toThrow('catalog/product mismatch');
    venue.load([market('BTCUSDT'), market('SOLUSDT')], [ticker('BTCUSDT', 100), ticker('SOLUSDT', 5)]);
    await vi.waitFor(() => side.client.validateMarkets([market('SOLUSDT')]));
    expect(side.failures).toEqual([]);
    side.client.dispose();
  });

  it('rejects subscriptions outside the announced catalog', async () => {
    const venue = restartableSource();
    venue.load([market('BTCUSDT')], [ticker('BTCUSDT', 100)]);
    const side = account(venue.source);
    await side.connected;
    side.client.send({ op: 'book', symbol: 'DOGEUSDT', subscribe: true });
    await vi.waitFor(() => expect(side.failures.map(String)).toEqual(['Error: Shared public feed symbol unavailable']));
  });
});
