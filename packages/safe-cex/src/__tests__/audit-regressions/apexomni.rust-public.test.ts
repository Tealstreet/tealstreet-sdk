import type { OrderBook } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniPublicWebsocket } from '../reference/apexomni/apexomni.ws-public';

const topic = 'orderBook200.H.BTCUSDT';
const fixture = () => {
  const parent = {
    isDisposed: false,
    name: 'apexomni',
    options: { publicDataOnly: true },
    emitter: { emit: vi.fn() },
    store: { update: vi.fn() },
    log: vi.fn(),
  };
  const socket = new ApexOmniPublicWebsocket(parent as any);
  const send = vi.fn();
  socket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
  const books: OrderBook[] = [];
  const stop = socket.listenOrderBook('BTCUSDT', (book) => {
    books.push({ asks: book.asks.map((l) => ({ ...l })), bids: book.bids.map((l) => ({ ...l })) });
  });
  send.mockClear();
  const frame = (type: string, u: number, a: unknown = [], b: unknown = []) => {
    for (const handler of socket.messageHandlers[topic]) handler({ topic, type, data: { u, a, b } });
  };
  return { socket, send, books, stop, frame };
};

afterEach(() => vi.restoreAllMocks());

describe('ApeX Omni depth continuity', () => {
  it('ignores deltas before a snapshot and duplicate or older cursors', () => {
    const { frame, books, stop } = fixture();
    frame('delta', 9, [], [['100', '1']]);
    expect(books).toEqual([]);
    frame('snapshot', 10, [['102', '2']], [['100', '1']]);
    frame('delta', 10, [], [['100', '99']]);
    frame('delta', 9, [], [['100', '99']]);
    expect(books).toHaveLength(1);
    frame('delta', 11, [], [['100', '3']]);
    expect(books.at(-1)?.bids).toEqual([{ price: 100, amount: 3, total: 3 }]);
    stop();
  });

  it('clears and resubscribes after a gap and accepts a reset snapshot', () => {
    const { frame, books, send, stop } = fixture();
    frame('snapshot', 30, [], [['100', '1']]);
    frame('delta', 32, [], [['100', '99']]);
    expect(books.at(-1)).toEqual({ asks: [], bids: [] });
    expect(send.mock.calls.map(([text]) => JSON.parse(text))).toEqual([
      { op: 'unsubscribe', args: [topic] },
      { op: 'subscribe', args: [topic] },
    ]);
    frame('delta', 33, [], [['100', '99']]);
    expect(books).toHaveLength(2);
    frame('snapshot', 1, [], [['101', '2']]);
    frame('delta', 2, [], [['101', '3']]);
    expect(books.at(-1)?.bids).toEqual([{ price: 101, amount: 3, total: 3 }]);
    stop();
  });

  it.each([
    [[['103', '1']], [['bad', '1']]],
    [[['103', '1']], [['100', '-1']]],
    [null, []],
    [[['103', '1', 'extra']], []],
    [[['0x10', '1']], []],
  ])('rejects malformed sides atomically and retains the preceding cursor: %j', (asks, bids) => {
    const { frame, books, stop } = fixture();
    frame('snapshot', 1, [['102', '2']], [['100', '1']]);
    frame('snapshot', 2, asks, bids);
    expect(books).toHaveLength(1);
    frame('delta', 2, [], [['100', '3']]);
    expect(books.at(-1)?.asks).toEqual([{ price: 102, amount: 2, total: 2 }]);
    expect(books.at(-1)?.bids).toEqual([{ price: 100, amount: 3, total: 3 }]);
    stop();
  });

  it('sorts snapshots and drops zero levels before applying deltas', () => {
    const { frame, books, stop } = fixture();
    frame(
      'snapshot',
      1,
      [
        ['104', '4'],
        ['103', '0'],
        ['102', '2'],
      ],
      [
        ['98', '2'],
        ['100', '1'],
      ],
    );
    expect(books.at(-1)?.asks).toEqual([
      { price: 102, amount: 2, total: 2 },
      { price: 104, amount: 4, total: 6 },
    ]);
    expect(books.at(-1)?.bids.map((l) => l.price)).toEqual([100, 98]);
    frame('delta', 2, [['102', '0']], [['100', '3']]);
    expect(books.at(-1)?.asks).toEqual([{ price: 104, amount: 4, total: 4 }]);
    stop();
  });

  it('waits for a new snapshot on socket reopen', () => {
    const { socket, frame, books, stop } = fixture();
    frame('snapshot', 1, [], [['100', '1']]);
    socket.onOpen();
    expect(books.at(-1)).toEqual({ asks: [], bids: [] });
    frame('delta', 2, [], [['100', '9']]);
    expect(books).toHaveLength(2);
    frame('snapshot', 1, [], [['101', '2']]);
    expect(books.at(-1)?.bids).toEqual([{ price: 101, amount: 2, total: 2 }]);
    stop();
  });
  it('clears on close before the unchanged base handler and emits no duplicate empty book on reopen', () => {
    const { socket, frame, books, send, stop } = fixture();
    const closeListeners: Array<() => void> = [];
    const raw = {
      readyState: WebSocket.OPEN,
      send,
      addEventListener: (event: string, callback: () => void) => {
        if (event === 'close') closeListeners.push(callback);
      },
    };
    vi.spyOn(socket as any, 'createWebsocket').mockReturnValue(raw);
    socket.connectAndSubscribe();
    expect(closeListeners).toHaveLength(2);
    expect(closeListeners[1]).toBe((socket as any).onClose);
    frame('snapshot', 10, [], [['100', '1']]);
    closeListeners[0]();
    expect(books.at(-1)).toEqual({ bids: [], asks: [] });
    socket.onOpen();
    expect(books).toHaveLength(2);
    frame('delta', 11, [], [['100', '99']]);
    expect(books).toHaveLength(2);
    stop();
  });
  it('does not mistake the nested ping request in a pong ACK for a server ping', () => {
    const { socket, send, stop } = fixture();
    vi.spyOn(socket, 'trackLastReceived').mockImplementation(() => undefined);
    const pong = vi.spyOn(socket, 'handlePongEvent').mockImplementation(() => undefined);
    socket.messageHandlers.pong = [() => socket.handlePongEvent()];
    socket.onMessage({
      data: JSON.stringify({ success: true, ret_msg: 'pong', conn_id: 'fixture', request: { op: 'ping', args: null } }),
    } as MessageEvent);
    expect(send).not.toHaveBeenCalled();
    expect(pong).toHaveBeenCalledOnce();
    socket.onMessage({ data: '{"op": "ping"}' } as MessageEvent);
    expect(send).toHaveBeenCalledOnce();
    expect(JSON.parse(send.mock.calls[0][0])).toMatchObject({ op: 'pong' });
    stop();
  });
});
