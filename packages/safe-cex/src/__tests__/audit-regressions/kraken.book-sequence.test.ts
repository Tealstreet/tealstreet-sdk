import type { KrakenExchange } from '../../exchanges/kraken/kraken.exchange';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { KrakenPublicWebsocket } from '../../exchanges/kraken/kraken.ws-public';

const SYMBOL = 'PF_XBTUSD';
const sockets: KrakenPublicWebsocket[] = [];
afterEach(() => sockets.splice(0).forEach((socket) => socket.dispose()));
const fixture = () => {
  const parent = {
    name: 'kraken', options: { testnet: false, publicDataOnly: true }, isDisposed: false,
    log: vi.fn(), emitAndLogError: vi.fn(), syncSocketHealthToStore: vi.fn(),
    emitter: { emit: vi.fn(), on: vi.fn(), off: vi.fn() }, getActiveTickerSymbols: () => [],
    store: { markets: [{ symbol: SYMBOL, contractSize: 0.5 }], tickers: [], tickersBySymbol: {}, update: vi.fn() },
  } as unknown as KrakenExchange;
  const socket = new KrakenPublicWebsocket(parent);
  sockets.push(socket);
  Object.defineProperty(socket, 'isConnected', { get: () => true });
  const send = vi.spyOn(socket, 'send').mockImplementation(() => {});
  const callback = vi.fn();
  const stop = socket.listenOrderBook(SYMBOL, callback);
  const message = (body: Record<string, unknown>) => socket.onMessage({ data: JSON.stringify({ product_id: SYMBOL, ...body }) } as MessageEvent);
  const snapshot = (seq: number, bids = [{ price: 99, qty: 2 }], asks = [{ price: 101, qty: 4 }]) =>
    message({ feed: 'book_snapshot', seq, bids, asks });
  const delta = (seq: number, qty: number, price = 99, side = 'buy') => message({ feed: 'book', seq, qty, price, side });
  return { socket, send, callback, stop, message, snapshot, delta };
};

describe('Kraken Futures complete sequenced book publication', () => {
  it('publishes every level, sorted and scaled, including updates beyond the old 100-level cut', () => {
    const { snapshot, delta, callback } = fixture();
    const bids = Array.from({ length: 125 }, (_, i) => ({ price: 1000 - i, qty: 2 }));
    const asks = Array.from({ length: 130 }, (_, i) => ({ price: 1001 + i, qty: 4 }));
    snapshot(100, bids.reverse(), asks.reverse());
    expect(callback).toHaveBeenCalledOnce();
    const book = callback.mock.calls[0][0];
    expect(book.bids).toHaveLength(125);
    expect(book.asks).toHaveLength(130);
    expect(book.bids.at(-1)).toEqual({ price: 876, amount: 1, total: 0 });
    expect(book.asks.at(-1)).toEqual({ price: 1130, amount: 2, total: 0 });
    delta(101, 6, 876);
    expect(callback.mock.calls[1][0].bids.at(-1)).toEqual({ price: 876, amount: 3, total: 0 });
    expect(book.bids.at(-1).amount).toBe(1);
  });

  it('ignores pre-snapshot, duplicate and older messages and accepts only contiguous updates', () => {
    const { snapshot, delta, callback } = fixture();
    delta(99, 99);
    expect(callback).not.toHaveBeenCalled();
    snapshot(100);
    delta(100, 90);
    delta(99, 91);
    snapshot(100, [{ price: 98, qty: 92 }]);
    snapshot(99, [{ price: 98, qty: 93 }]);
    expect(callback).toHaveBeenCalledOnce();
    delta(101, 0);
    expect(callback.mock.calls[1][0].bids).toEqual([]);
    delta(102, 8, 102, 'sell');
    expect(callback.mock.calls[2][0].asks).toEqual([
      { price: 101, amount: 2, total: 0 }, { price: 102, amount: 4, total: 0 },
    ]);
  });

  it('resubscribes once on a gap and retains the last published snapshot until recovery', () => {
    const { snapshot, delta, callback, send } = fixture();
    snapshot(100);
    const published = callback.mock.calls[0][0];
    send.mockClear();
    delta(103, 90);
    expect(callback).toHaveBeenCalledOnce();
    expect(send.mock.calls.map(([text]) => JSON.parse(text))).toEqual([
      { event: 'unsubscribe', feed: 'book', product_ids: [SYMBOL] },
      { event: 'subscribe', feed: 'book', product_ids: [SYMBOL] },
    ]);
    delta(104, 91);
    expect(send).toHaveBeenCalledTimes(2);
    expect(callback).toHaveBeenCalledOnce();
    expect(published.bids).toEqual([{ price: 99, amount: 1, total: 0 }]);
    snapshot(105, [{ price: 98, qty: 6 }]);
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callback.mock.calls[1][0].bids).toEqual([{ price: 98, amount: 3, total: 0 }]);
    delta(106, 8, 98);
    expect(callback.mock.calls[2][0].bids).toEqual([{ price: 98, amount: 4, total: 0 }]);
    expect(published.bids).toEqual([{ price: 99, amount: 1, total: 0 }]);
  });

  it('requires a fresh snapshot after reconnect and after the last listener is removed', () => {
    const { socket, snapshot, delta, callback, stop } = fixture();
    snapshot(100);
    const published = callback.mock.calls[0][0];
    socket.onOpen();
    delta(101, 90);
    expect(callback).toHaveBeenCalledOnce();
    snapshot(10);
    delta(11, 6);
    expect(callback).toHaveBeenCalledTimes(3);
    expect(published.bids).toEqual([{ price: 99, amount: 1, total: 0 }]);
    stop();
    const next = vi.fn();
    socket.listenOrderBook(SYMBOL, next);
    delta(12, 90);
    expect(next).not.toHaveBeenCalled();
    snapshot(1);
    expect(next).toHaveBeenCalledOnce();
  });

  it('rejects malformed snapshots atomically and malformed deltas without advancing the cursor', () => {
    const { message, snapshot, delta, callback } = fixture();
    snapshot(100);
    const published = callback.mock.calls[0][0];
    for (const body of [
      { bids: [{ price: 98, qty: 7 }], asks: null },
      { bids: [{ price: 98, qty: 7 }, { price: 0, qty: 2 }], asks: [] },
      { bids: [{ price: 98, qty: -1 }], asks: [] },
      { bids: [], asks: [{ price: 'bad', qty: 2 }] },
    ]) message({ feed: 'book_snapshot', seq: 101, ...body });
    delta(101, -1);
    delta(101, 2, 99, 'unknown');
    expect(callback).toHaveBeenCalledOnce();
    delta(101, 6);
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callback.mock.calls[1][0].bids).toEqual([{ price: 99, amount: 3, total: 0 }]);
    expect(published.bids).toEqual([{ price: 99, amount: 1, total: 0 }]);
  });
});
