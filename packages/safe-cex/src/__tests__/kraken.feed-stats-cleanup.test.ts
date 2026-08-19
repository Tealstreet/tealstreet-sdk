import { describe, expect, it, vi } from 'vitest';

import { KrakenPublicWebsocket } from '../exchanges/kraken/kraken.ws-public';

const createMockParent = () =>
  ({
    name: 'kraken',
    options: { testnet: false, publicDataOnly: true, key: '' },
    isDisposed: false,
    log: vi.fn(),
    emitAndLogError: vi.fn(),
    emitter: { emit: vi.fn(), on: vi.fn(), off: vi.fn() },
    syncSocketHealthToStore: vi.fn(),
    store: { markets: [], tickers: [], tickersBySymbol: {}, update: vi.fn(), updateTicker: vi.fn() },
  }) as any;

const createSocket = () => {
  const ws = new KrakenPublicWebsocket(createMockParent()) as any;
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
});
