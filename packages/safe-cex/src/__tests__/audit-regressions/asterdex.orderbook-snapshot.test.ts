import { describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { DefaultStore } from '../../store/store.base';
import type { OrderBook } from '../../types';

const symbol = 'BTCUSDT';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    tickers: [
      {
        id: symbol,
        symbol,
        bid: 0,
        ask: 0,
        last: 100,
        mark: 100,
        index: 100,
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        openInterest: 0,
        volume: 0,
        quoteVolume: 0,
      },
    ] as any,
  });
  const exchange = new AsterDEXExchange('asterdex', { key: 'test-key', secret: 'test-secret' }, store);
  return { exchange, store, websocket: exchange.publicWebsocket };
};

const snapshotFrame = (stream: string, bids: unknown, asks: unknown) => ({
  data: JSON.stringify({
    stream,
    data: { e: 'depthUpdate', E: 1, T: 1, s: symbol, U: 1, u: 1, pu: 0, b: bids, a: asks },
  }),
});

const orderBookStream = 'btcusdt@depth20@100ms';

const framesFor = (frames: string[], method: 'SUBSCRIBE' | 'UNSUBSCRIBE') =>
  frames
    .map((frame) => JSON.parse(frame))
    .filter((frame) => frame.method === method && frame.params.includes(orderBookStream));

describe('audit regression: AsterDEX partial orderbook snapshots', () => {
  it('subscribes to the documented partial snapshot stream and publishes it once', () => {
    const { store, websocket } = createExchange();
    const frames: string[] = [];
    const callback = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send: (frame: string) => frames.push(frame) } as unknown as WebSocket;

    websocket.listenOrderBook(symbol, callback);
    websocket.onMessage(snapshotFrame('btcusdt@depth20@100ms', [['99', '2'], ['100', '1']], [['102', '4'], ['101', '3']]) as MessageEvent);

    expect(frames.map((frame) => JSON.parse(frame))).toContainEqual({
      method: 'SUBSCRIBE',
      params: ['btcusdt@depth20@100ms'],
      id: 1,
    });
    expect(callback).toHaveBeenCalledTimes(1);
    expect(callback).toHaveBeenCalledWith({
      bids: [
        { price: 100, amount: 1, total: 1 },
        { price: 99, amount: 2, total: 3 },
      ],
      asks: [
        { price: 101, amount: 3, total: 3 },
        { price: 102, amount: 4, total: 7 },
      ],
    });
    expect(store.tickersBySymbol[symbol]).toMatchObject({ bid: 100, ask: 101 });
  });

  it('fails closed for diff, zero-quantity, and crossed frames instead of manufacturing a book', () => {
    const { store, websocket } = createExchange();
    const callback = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as unknown as WebSocket;
    websocket.listenOrderBook(symbol, callback);

    websocket.onMessage(snapshotFrame('btcusdt@depth20@100ms', [['100', '1']], [['101', '1']]) as MessageEvent);
    const lastGoodBook = callback.mock.calls[0][0];

    websocket.onMessage(snapshotFrame('btcusdt@depth@500ms', [['100', '0']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame('btcusdt@depth20@100ms', [['102', '1']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame('btcusdt@depth20@100ms', [['100', '0']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame('btcusdt@depth20@100ms', [['100junk', '1']], [['101', '1']]) as MessageEvent);

    expect(callback).toHaveBeenCalledTimes(1);
    expect(callback.mock.calls[0][0]).toBe(lastGoodBook);
    expect(store.tickersBySymbol[symbol]).toMatchObject({ bid: 100, ask: 101 });
  });

  it('requires documented string pairs for every snapshot level', () => {
    const { websocket } = createExchange();
    const callback = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as unknown as WebSocket;
    websocket.listenOrderBook(symbol, callback);

    websocket.onMessage(snapshotFrame(orderBookStream, [['100', '1']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame(orderBookStream, [[true, '1']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame(orderBookStream, [[['100'], '1']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame(orderBookStream, [[{ value: '100' }, '1']], [['101', '1']]) as MessageEvent);
    websocket.onMessage(snapshotFrame(orderBookStream, [['100', ['1']]], [['101', '1']]) as MessageEvent);

    expect(callback).toHaveBeenCalledTimes(1);
  });

  it('uses a reserved ping ID that cannot collide with subscription acknowledgements', () => {
    const { websocket } = createExchange();
    const frames: string[] = [];
    websocket.ws = { readyState: WebSocket.OPEN, send: (frame: string) => frames.push(frame) } as unknown as WebSocket;

    websocket.listenOrderBook(symbol, vi.fn());
    websocket.ping();

    const requests = frames.map((frame) => JSON.parse(frame));
    expect(requests).toContainEqual({ method: 'SUBSCRIBE', params: [orderBookStream], id: 1 });
    expect(requests).toContainEqual({ method: 'LIST_SUBSCRIPTIONS', id: -1 });
    expect(requests.filter((request) => request.method === 'SUBSCRIBE').every((request) => request.id !== -1)).toBe(true);
  });

  it('shares one upstream stream, isolates listener snapshots, and tears down only after the final unsubscribe', () => {
    const { websocket } = createExchange();
    const frames: string[] = [];
    const first = vi.fn((orderBook: OrderBook) => {
      orderBook.bids[0].price = 0;
    });
    const second = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send: (frame: string) => frames.push(frame) } as unknown as WebSocket;

    const unsubscribeFirst = websocket.listenOrderBook(symbol, first);
    const unsubscribeSecond = websocket.listenOrderBook(symbol.toLowerCase(), second);

    expect(framesFor(frames, 'SUBSCRIBE')).toHaveLength(1);

    websocket.onMessage(snapshotFrame(orderBookStream, [['100', '1']], [['101', '1']]) as MessageEvent);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    expect(second.mock.calls[0][0]).toMatchObject({ bids: [{ price: 100 }], asks: [{ price: 101 }] });

    unsubscribeFirst();
    expect(framesFor(frames, 'UNSUBSCRIBE')).toHaveLength(0);
    websocket.onMessage(snapshotFrame(orderBookStream, [['100', '1']], [['101', '1']]) as MessageEvent);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(2);

    unsubscribeSecond();
    expect(framesFor(frames, 'UNSUBSCRIBE')).toHaveLength(1);
    expect((websocket as any).topics[orderBookStream]).toBeUndefined();
  });

  it('keeps duplicate callback registrations independent', () => {
    const { websocket } = createExchange();
    const frames: string[] = [];
    const callback = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send: (frame: string) => frames.push(frame) } as unknown as WebSocket;

    const unsubscribeFirst = websocket.listenOrderBook(symbol, callback);
    const unsubscribeSecond = websocket.listenOrderBook(symbol, callback);
    websocket.onMessage(snapshotFrame(orderBookStream, [['100', '1']], [['101', '1']]) as MessageEvent);
    expect(callback).toHaveBeenCalledTimes(2);

    unsubscribeFirst();
    websocket.onMessage(snapshotFrame(orderBookStream, [['100', '1']], [['101', '1']]) as MessageEvent);
    expect(callback).toHaveBeenCalledTimes(3);
    expect(framesFor(frames, 'UNSUBSCRIBE')).toHaveLength(0);

    unsubscribeSecond();
    expect(framesFor(frames, 'UNSUBSCRIBE')).toHaveLength(1);
  });

  it('restores only active orderbook topics after reconnect and disposes their listeners', async () => {
    const { websocket } = createExchange();
    const frames: string[] = [];
    const callback = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send: (frame: string) => frames.push(frame) } as unknown as WebSocket;
    const unsubscribe = websocket.listenOrderBook(symbol, callback);

    websocket.onOpen();
    await Promise.resolve();
    expect(framesFor(frames, 'SUBSCRIBE')).toHaveLength(2);

    unsubscribe();
    websocket.onOpen();
    await Promise.resolve();
    expect(framesFor(frames, 'SUBSCRIBE')).toHaveLength(2);

    websocket.dispose();
    websocket.onMessage(snapshotFrame(orderBookStream, [['100', '1']], [['101', '1']]) as MessageEvent);
    expect(callback).not.toHaveBeenCalled();
    expect((websocket as any).orderBookSubscriptions.size).toBe(0);
    expect((websocket as any).topics).toEqual({});
  });
});
