import type { OrderBook } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MexcExchange } from '../reference/mexc/mexc.exchange';
import { normalizeMarket } from '../reference/mexc/mexc.utils';
import { DefaultStore } from '../../store/store.base';

type Snapshot = OrderBook & { lastUpdateId?: string };

const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

const snapshot = (version: string, bids: [number, number][] = [[100, 0.001]]): Snapshot => ({
  lastUpdateId: version,
  bids: bids.map(([price, amount]) => ({ price, amount, total: 0 })),
  asks: [{ price: 102, amount: 0.002, total: 0 }],
});

const settle = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

describe('MEXC incremental futures depth', () => {
  const exchanges: MexcExchange[] = [];
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    exchanges.forEach((exchange) => exchange.dispose());
    exchanges.length = 0;
    vi.useRealTimers();
  });

  const setup = () => {
    const exchange = new MexcExchange('mexc', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    exchanges.push(exchange);
    exchange.store.update({
      markets: [
        normalizeMarket({
          symbol: 'BTC_USDT',
          baseCoin: 'BTC',
          quoteCoin: 'USDT',
          contractSize: 0.0001,
        }),
      ],
    });
    const socket = { readyState: 1, send: vi.fn(), close: vi.fn(), removeEventListener: vi.fn() };
    const ws = exchange.publicWebsocket;
    ws.ws = socket as any;
    const books: OrderBook[] = [];
    const listener = vi.fn((book: OrderBook) => books.push(structuredClone(book)));
    const depth = (version: string | number, bids: unknown[] = [], asks: unknown[] = []) =>
      ws.onMessage({
        data: JSON.stringify({ channel: 'push.depth', symbol: 'BTC_USDT', data: { version, bids, asks } }),
      } as MessageEvent);
    const messages = () => socket.send.mock.calls.map(([text]) => JSON.parse(text));
    return { exchange, ws, socket, books, listener, depth, messages };
  };

  it.each([
    { success: true, code: 0, data: { version: 10, bids: [['0x64', 10]], asks: [] } },
    { success: false, code: 510, data: { version: 10, bids: [], asks: [] } },
    { success: true, code: 0, data: { version: 10, bids: [[100, 10], [99]], asks: [] } },
    { success: true, code: 0, data: { version: 10, bids: [[100, 'invalid']], asks: [] } },
    { success: true, code: 0, data: { version: 10, bids: [[100, null]], asks: [] } },
  ])('rejects an invalid raw REST snapshot before normalization can discard its rows', async (data) => {
    const { exchange } = setup();
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data }) } as any;
    await expect(exchange.fetchOrderBook('BTCUSDT', 1000)).rejects.toThrow();
  });

  it('subscribes before the 1000-level REST read and keeps one subscription until the final listener leaves', async () => {
    const { exchange, ws, listener, messages } = setup();
    const response = deferred<Snapshot>();
    const fetch = vi.spyOn(exchange, 'fetchOrderBook').mockImplementation((symbol, limit) => {
      expect(messages()).toEqual([{ method: 'sub.depth', param: { symbol: 'BTC_USDT', compress: false } }]);
      expect([symbol, limit]).toEqual(['BTCUSDT', 1000]);
      return response.promise;
    });
    const first = ws.listenOrderBook('BTCUSDT', listener);
    const second = ws.listenOrderBook('BTCUSDT', listener);
    expect(fetch).toHaveBeenCalledTimes(1);
    response.resolve(snapshot('10'));
    await settle();
    expect(listener).toHaveBeenCalledTimes(2);
    first();
    first();
    expect(messages()).toHaveLength(1);
    second();
    expect(messages().map((message) => message.method)).toEqual(['sub.depth', 'unsub.depth']);
  });

  it('replays buffered updates, preserves deeper levels, deletes zero volumes and ignores old versions', async () => {
    const { exchange, ws, books, listener, depth } = setup();
    const response = deferred<Snapshot>();
    vi.spyOn(exchange, 'fetchOrderBook').mockReturnValue(response.promise);
    ws.listenOrderBook('BTCUSDT', listener);
    depth(9, [[100, 999, 1]]);
    depth(11, [
      [100, 0, 0],
      [101, 30, 1],
    ]);
    expect(books).toHaveLength(0);
    response.resolve(
      snapshot(
        '10',
        Array.from({ length: 25 }, (_, i) => [100 - i, 0.001]),
      ),
    );
    await settle();
    expect(books).toHaveLength(1);
    expect(books[0].bids).toHaveLength(25);
    expect(books[0].bids[0]).toEqual({ price: 101, amount: 0.003, total: 0.003 });
    expect(books[0].bids.at(-1)?.price).toBe(76);
    expect(books[0].asks[0].amount).toBe(0.002);
    depth(11, [[101, 999, 1]]);
    depth(10, [[101, 999, 1]]);
    expect(books).toHaveLength(1);
    depth(12, [[99, 20, 1]]);
    expect(books).toHaveLength(2);
    expect(books[1].bids[1].amount).toBe(0.002);
  });

  it('suppresses a discontinuous buffered book and waits for a fresh REST anchor with backoff', async () => {
    const { exchange, ws, books, listener, depth } = setup();
    const response = deferred<Snapshot>();
    const fetch = vi
      .spyOn(exchange, 'fetchOrderBook')
      .mockReturnValueOnce(response.promise)
      .mockResolvedValue(snapshot('13', [[103, 0.001]]));
    ws.listenOrderBook('BTCUSDT', listener);
    depth(13, [[101, 99, 1]]);
    response.resolve(snapshot('10'));
    await settle();
    expect(books).toHaveLength(0);
    depth(14, [[101, 99, 1]]);
    await vi.advanceTimersByTimeAsync(499);
    expect(fetch).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(books).toHaveLength(1);
    expect(books[0].bids[0].price).toBe(103);
    depth(14, [[104, 20, 1]]);
    expect(books.at(-1)?.bids[0].price).toBe(104);
  });

  it('rejects an old snapshot after final unsubscribe and resubscribe', async () => {
    const { exchange, ws, books, listener } = setup();
    const old = deferred<Snapshot>();
    const current = deferred<Snapshot>();
    vi.spyOn(exchange, 'fetchOrderBook').mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const leave = ws.listenOrderBook('BTCUSDT', listener);
    leave();
    ws.listenOrderBook('BTCUSDT', listener);
    old.resolve(snapshot('10', [[90, 0.001]]));
    await settle();
    expect(books).toHaveLength(0);
    current.resolve(snapshot('20', [[100, 0.001]]));
    await settle();
    expect(books).toHaveLength(1);
    expect(books[0].bids[0].price).toBe(100);
  });

  it('invalidates in-flight snapshots on close and resubscribes before the reconnect snapshot', async () => {
    const { exchange, ws, socket, listener, books, messages } = setup();
    const old = deferred<Snapshot>();
    const current = deferred<Snapshot>();
    const fetch = vi
      .spyOn(exchange, 'fetchOrderBook')
      .mockReturnValueOnce(old.promise)
      .mockImplementationOnce(() => {
        expect(messages().filter((message) => message.method === 'sub.depth')).toHaveLength(2);
        return current.promise;
      });
    vi.spyOn(ws, 'onClose').mockImplementation(() => undefined);
    ws.listenOrderBook('BTCUSDT', listener);
    socket.readyState = 3;
    (ws as any).handleClose({ code: 1006 });
    old.resolve(snapshot('10'));
    await settle();
    expect(books).toHaveLength(0);
    socket.readyState = 1;
    ws.onOpen();
    expect(fetch).toHaveBeenCalledTimes(2);
    current.resolve(snapshot('100', [[105, 0.001]]));
    await settle();
    expect(books[0].bids[0].price).toBe(105);
  });

  it('bounds snapshot buffers and ignores the superseded response after overflow', async () => {
    const { exchange, ws, listener, depth, books } = setup();
    const old = deferred<Snapshot>();
    const fetch = vi
      .spyOn(exchange, 'fetchOrderBook')
      .mockReturnValueOnce(old.promise)
      .mockResolvedValue(snapshot('3000', [[105, 0.001]]));
    ws.listenOrderBook('BTCUSDT', listener);
    for (let i = 0; i < 2001; i++) depth(i + 1);
    old.resolve(snapshot('10'));
    await settle();
    expect(books).toHaveLength(0);
    expect((ws as any).depthStates.get('BTCUSDT').buffer).toBeUndefined();
    await vi.advanceTimersByTimeAsync(500);
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(books[0].bids[0].price).toBe(105);
  });

  it('keeps versions above the JS integer limit exact', async () => {
    const { exchange, ws, listener, books, depth } = setup();
    vi.spyOn(exchange, 'fetchOrderBook').mockResolvedValue(snapshot('9007199254740993'));
    ws.listenOrderBook('BTCUSDT', listener);
    await settle();
    depth('9007199254740994', [[101, 30, 1]]);
    depth('9007199254740994', [[101, 90, 1]]);
    expect(books).toHaveLength(2);
    expect(books.at(-1)?.bids[0].amount).toBe(0.003);
  });

  it.each([null, true, '', 'NaN', -1])(
    'rejects malformed contract volume %s without deleting an existing level',
    async (volume) => {
      const { exchange, ws, listener, books, depth } = setup();
      const fetch = vi.spyOn(exchange, 'fetchOrderBook').mockResolvedValue(snapshot('10'));
      ws.listenOrderBook('BTCUSDT', listener);
      await settle();
      depth(11, [[100, volume, 1]]);
      expect(books).toHaveLength(1);
      expect(books[0].bids[0].amount).toBe(0.001);
      await vi.advanceTimersByTimeAsync(500);
      expect(fetch).toHaveBeenCalledTimes(2);
    },
  );

  it('does not fetch or replay a phantom depth subscription after unsubscribe before connection', () => {
    const { exchange, ws, socket, listener, messages } = setup();
    const fetch = vi.spyOn(exchange, 'fetchOrderBook');
    socket.readyState = 0;
    const leave = ws.listenOrderBook('BTCUSDT', listener);
    expect(fetch).not.toHaveBeenCalled();
    expect(messages()).toEqual([]);
    leave();
    socket.readyState = 1;
    ws.onOpen();
    expect(messages().some((message) => message.method === 'sub.depth')).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('cancels a failed snapshot retry on unsubscribe and ignores responses after disposal', async () => {
    const { exchange, ws, listener } = setup();
    const response = deferred<Snapshot>();
    const fetch = vi
      .spyOn(exchange, 'fetchOrderBook')
      .mockRejectedValueOnce(new Error('network reset'))
      .mockReturnValueOnce(response.promise);
    const leave = ws.listenOrderBook('BTCUSDT', listener);
    await settle();
    leave();
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetch).toHaveBeenCalledTimes(1);
    ws.listenOrderBook('BTCUSDT', listener);
    exchange.dispose();
    response.resolve(snapshot('10'));
    await settle();
    expect(listener).not.toHaveBeenCalled();
  });
});
