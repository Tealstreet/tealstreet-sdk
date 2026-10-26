import { describe, expect, it, vi } from 'vitest';

import { KrakenPublicWebsocket } from './reference/kraken/kraken.ws-public';

const createMockParent = (overrides: Record<string, unknown> = {}) => {
  const parent = {
    name: 'kraken',
    options: { testnet: false, publicDataOnly: true, key: '' },
    isDisposed: false,
    log: vi.fn(),
    emitAndLogError: vi.fn(),
    emitter: { emit: vi.fn(), on: vi.fn(), off: vi.fn() },
    syncSocketHealthToStore: vi.fn(),
    getActiveTickerSymbols: vi.fn(() => []),
    store: { markets: [], tickers: [], tickersBySymbol: {}, update: vi.fn(), updateTicker: vi.fn() },
    ...overrides,
  };
  return parent as any;
};

const createSocket = (parentOverrides: Record<string, unknown> = {}) => {
  const ws = new KrakenPublicWebsocket(createMockParent(parentOverrides)) as any;
  vi.spyOn(ws, 'createWebsocket').mockReturnValue({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  });
  ws.send = vi.fn();
  Object.defineProperty(ws, 'isConnected', { get: () => true, configurable: true });
  return ws;
};

const bookDeltaFrame = (symbol: string) =>
  JSON.stringify({ feed: 'book', product_id: symbol, side: 'buy', price: 1, qty: 1 });

const bookSnapshotFrame = (symbol: string) =>
  JSON.stringify({
    feed: 'book_snapshot',
    product_id: symbol,
    bids: [{ price: 1, qty: 1 }],
    asks: [{ price: 2, qty: 1 }],
  });

describe('kraken book feed stats', () => {
  // Kraken keeps orderbook subscriptions in a private `orderBookHandlers` map and
  // never touches `messageHandlers`, so the `book` stats key matches no handler
  // key and base.ws.ts's reconcile can never mark it idle. The disposer clearing
  // it is therefore the only escape — and it only works if an in-flight frame
  // cannot immediately put the entry back.
  it('clears the book entry when the last orderbook listener goes', () => {
    const ws = createSocket();

    const dispose = ws.listenOrderBook('PI_XBTUSD', () => undefined);
    ws.orderBookCache = { PI_XBTUSD: { bids: new Map(), asks: new Map() } };
    ws.onMessage({ data: bookDeltaFrame('PI_XBTUSD') });
    expect(ws.messageHandlersStats.book).toBeDefined();

    dispose();

    expect(ws.messageHandlersStats.book).toBeUndefined();
  });

  it('does not resurrect the entry from frames that arrive after teardown', () => {
    const ws = createSocket();

    const dispose = ws.listenOrderBook('PI_XBTUSD', () => undefined);
    ws.orderBookCache = { PI_XBTUSD: { bids: new Map(), asks: new Map() } };
    ws.onMessage({ data: bookDeltaFrame('PI_XBTUSD') });
    dispose();

    // Kraken Futures is never sent an unsubscribe, so book deltas keep arriving
    // forever. Recording them unconditionally in onMessage re-created the entry
    // with hadHandlers:false and lastCallbacksCount:1 — permanently unreconcilable
    // — which made the cleanup a no-op within milliseconds.
    ws.onMessage({ data: bookDeltaFrame('PI_XBTUSD') });

    expect(ws.messageHandlersStats.book).toBeUndefined();
  });

  it('records the initial book snapshot as a live book feed', () => {
    const ws = createSocket();

    ws.listenOrderBook('PI_XBTUSD', () => undefined);
    ws.onMessage({ data: bookSnapshotFrame('PI_XBTUSD') });

    expect(ws.messageHandlersStats.book).toBeDefined();
  });

  it('does not resurrect the entry from snapshots that arrive after teardown', () => {
    const ws = createSocket();

    const dispose = ws.listenOrderBook('PI_XBTUSD', () => undefined);
    ws.onMessage({ data: bookSnapshotFrame('PI_XBTUSD') });
    dispose();
    ws.onMessage({ data: bookSnapshotFrame('PI_XBTUSD') });

    expect(ws.messageHandlersStats.book).toBeUndefined();
  });

  it('keeps the entry while another symbol is still listening', () => {
    const ws = createSocket();

    const disposeBtc = ws.listenOrderBook('PI_XBTUSD', () => undefined);
    ws.listenOrderBook('PI_ETHUSD', () => undefined);
    ws.orderBookCache = {
      PI_XBTUSD: { bids: new Map(), asks: new Map() },
      PI_ETHUSD: { bids: new Map(), asks: new Map() },
    };
    ws.onMessage({ data: bookDeltaFrame('PI_XBTUSD') });

    disposeBtc();

    expect(ws.messageHandlersStats.book).toBeDefined();
  });

  it('subscribes only active ticker products when the exchange supplies them', () => {
    const ws = createSocket({
      getActiveTickerSymbols: vi.fn(() => ['PI_ETHUSD']),
      store: {
        markets: [{ symbol: 'PI_XBTUSD' }, { symbol: 'PI_ETHUSD' }, { symbol: 'PI_SOLUSD' }],
        tickers: [],
        tickersBySymbol: {},
        update: vi.fn(),
        updateTicker: vi.fn(),
      },
    });

    ws.onOpen();

    expect(ws.send).toHaveBeenCalledWith(
      JSON.stringify({ event: 'subscribe', feed: 'ticker', product_ids: ['PI_ETHUSD'] }),
    );
  });

  it('falls back to all ticker products when the active list misses every market', () => {
    const ws = createSocket({
      getActiveTickerSymbols: vi.fn(() => ['NOT_ON_KRAKEN']),
      store: {
        markets: [{ symbol: 'PI_XBTUSD' }, { symbol: 'PI_ETHUSD' }],
        tickers: [],
        tickersBySymbol: {},
        update: vi.fn(),
        updateTicker: vi.fn(),
      },
    });

    ws.onOpen();

    expect(ws.send).toHaveBeenCalledWith(
      JSON.stringify({ event: 'subscribe', feed: 'ticker', product_ids: ['PI_XBTUSD', 'PI_ETHUSD'] }),
    );
  });
});
