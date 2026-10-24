import { afterEach, describe, expect, it, vi } from 'vitest';

import { HyrotraderExchange } from '../../exchanges/bybit/hyrotrader.exchange';
import { DefaultStore } from '../../store/store.base';
import { Order, OrderSide, OrderType, Position, PositionMode, PositionSide } from '../../types';

class TestHyroExchange extends HyrotraderExchange {
  getBybitRestBaseUrl() {
    return 'https://simulator.test';
  }
  getBybitPrivateWsUrl() {
    return 'wss://simulator.test/v5/private';
  }
}
const exchanges: TestHyroExchange[] = [];
const setup = () => {
  const store = new DefaultStore();
  const exchange = new TestHyroExchange('hyrotrader', { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  exchanges.push(exchange);
  return { exchange, store };
};
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe('HyroTrader authoritative private snapshots', () => {
  it.each([undefined, ''])('rejects a position with unusable leverage %s', async (leverage) => {
    const { exchange } = setup();
    exchange.xhr = {
      get: vi
        .fn()
        .mockResolvedValue({
          data: {
            retCode: 0,
            retMsg: 'OK',
            result: {
              list: [
                {
                  symbol: 'BTCUSDT',
                  size: '1',
                  side: 'Buy',
                  avgPrice: '50000',
                  markPrice: '51000',
                  unrealisedPnl: '1000',
                  leverage,
                },
              ],
            },
          },
        }),
    } as any;
    await expect(exchange.fetchPositions()).rejects.toThrow('leverage');
  });

  it.each(['orders', 'positions'])('rejects a repeating %s cursor', async (kind) => {
    const { exchange } = setup();
    const get = vi
      .fn()
      .mockResolvedValue({ data: { retCode: 0, retMsg: 'OK', result: { list: [], nextPageCursor: 'cycle' } } });
    exchange.xhr = { get } as any;
    await expect(kind === 'orders' ? exchange.fetchOrders() : exchange.fetchPositions()).rejects.toThrow('cursor');
    expect(get).toHaveBeenCalledTimes(2);
  });
  it.each(['side', 'unrealisedPnl'])('rejects a nonzero position with invalid %s', async (field) => {
    const { exchange } = setup();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          retCode: 0,
          retMsg: 'OK',
          result: {
            list: [
              {
                symbol: 'BTCUSDT',
                size: '1',
                leverage: '10',
                side: 'Buy',
                avgPrice: '50000',
                markPrice: '51000',
                unrealisedPnl: '1000',
                [field]: 'invalid',
              },
            ],
          },
        },
      }),
    } as any;
    await expect(exchange.fetchPositions()).rejects.toThrow(/missing side, price|invalid numeric/);
  });
  it.each([{ size: '0', unrealisedPnl: 'bad' }, { size: '-1' }, { size: ' ' }, { size: '0', unrealisedPnl: true }])(
    'rejects invalid flat or negative positions',
    async (row) => {
      const { exchange } = setup();
      exchange.xhr = {
        get: vi
          .fn()
          .mockResolvedValue({ data: { retCode: 0, retMsg: 'OK', result: { list: [{ symbol: 'BTCUSDT', ...row }] } } }),
      } as any;
      await expect(exchange.fetchPositions()).rejects.toThrow();
    },
  );
  it('rejects incomplete open orders before publishing a snapshot', async () => {
    const { exchange, store } = setup();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: { retCode: 0, retMsg: 'OK', result: { list: [{ orderId: 'bad', symbol: 'BTCUSDT' }] } },
      }),
    } as any;
    const update = vi.spyOn(store, 'update');
    await expect(exchange.fetchOrders()).rejects.toThrow('invalid order fields');
    expect(update).not.toHaveBeenCalled();
  });
  it.each([
    { retCode: 10000, retMsg: 'Request deadline exceeded' },
    { retCode: 0, retMsg: 'OK', result: {} },
    { retCode: 0, retMsg: 'OK', result: { list: [{}] } },
  ])('rejects failed or malformed account-wide position snapshots', async (data) => {
    const { exchange } = setup();
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data }) } as any;
    await expect(exchange.fetchPositions()).rejects.toThrow();
  });
  it('rejects failed recovery without publishing any sibling snapshot', async () => {
    const { exchange, store } = setup();
    const old = { id: 'old' } as Order;
    store.update({ orders: [old] });
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([{ id: 'new' } as Order]);
    vi.spyOn(exchange, 'fetchPositions').mockRejectedValue(new Error('positions unavailable'));
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue([
      { accountType: 'UNIFIED', totalEquity: '1000', coin: [] },
    ]);
    await expect(exchange.fetchAndSetPrivateSnapshot()).rejects.toThrow('positions unavailable');
    expect(store.orders).toEqual([old]);
  });
  it('publishes one complete snapshot using the new positions for wallet upnl', async () => {
    const { exchange, store } = setup();
    store.update({ positions: [{ id: 'old', unrealizedPnl: 10 } as Position] });
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([{ id: 'new', unrealizedPnl: 20 } as Position]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue([
      { accountType: 'UNIFIED', totalEquity: '1000', coin: [] },
    ]);
    const update = vi.spyOn(store, 'update');
    await exchange.fetchAndSetPrivateSnapshot();
    expect(update).toHaveBeenCalledTimes(1);
    expect(store.balance).toMatchObject({ total: 980, upnl: 20 });
    expect(store.positions.map(({ id }) => id)).toEqual(['new']);
  });
  it('discards a poll that resolves after newer private events or a new session', async () => {
    const { exchange, store } = setup();
    let resolve!: (value: Order[]) => void;
    vi.spyOn(exchange, 'fetchOrders').mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchRawBalance').mockResolvedValue([
      { accountType: 'UNIFIED', totalEquity: '1000', coin: [] },
    ]);
    const pending = exchange.reconcilePrivateState();
    exchange.privateWebsocket.stateRevision++;
    store.update({ orders: [{ id: 'newer' } as Order] });
    resolve([{ id: 'stale' } as Order]);
    expect(await pending).toBe(false);
    expect(store.orders.map(({ id }) => id)).toEqual(['newer']);
  });
});

const message = (value: unknown) => new MessageEvent('message', { data: JSON.stringify(value) });
const attach = (exchange: TestHyroExchange) => {
  const socket = exchange.privateWebsocket;
  const send = vi.fn();
  socket.ws = { readyState: WebSocket.OPEN, send, close: vi.fn(), removeEventListener: vi.fn() } as any;
  const frames = () => send.mock.calls.map(([text]) => JSON.parse(text));
  const ack = (op: string, success = true, req_id = frames().find((frame) => frame.op === op)?.req_id) =>
    socket.onMessage(message({ op, req_id, success, ret_msg: success ? '' : 'API key rejected' }));
  return { socket, send, frames, ack };
};

describe('HyroTrader private stream recovery ordering', () => {
  it('accepts zero-size flat position updates without leverage after a close', () => {
    const { exchange, store } = setup();
    store.update({
      positions: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          side: PositionSide.Short,
          entryPrice: 83000,
          notional: 165.75,
          leverage: 3,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 0.002,
          liquidationPrice: 0,
          positionMode: PositionMode.OneWay,
        } as Position,
      ],
    });
    const { socket } = attach(exchange);
    socket.ready = true;
    socket.onMessage(
      message({
        creationTime: 1792636946033,
        topic: 'position',
        data: [
          {
            positionIdx: 0,
            positionValue: '0',
            side: '',
            size: '0',
            symbol: 'BTCUSDT',
            updatedTime: '1792636946033',
          },
        ],
      }),
    );
    expect(exchange.emitAndLogError).not.toHaveBeenCalled();
    expect(store.positions[0]).toMatchObject({ id: 'BTCUSDT', contracts: 0, notional: 0, leverage: 3 });
  });

  it('accepts entryPrice as the position entry-price field', () => {
    const { exchange, store } = setup();
    const { socket } = attach(exchange);
    socket.ready = true;
    socket.onMessage(
      message({
        topic: 'position',
        data: [
          {
            symbol: 'BTCUSDT',
            size: '0.002',
            side: 'Buy',
            entryPrice: '82829.30',
            markPrice: '82830.07',
            unrealisedPnl: '0.00154',
            positionValue: '165.66014',
            leverage: '3',
            positionIdx: 0,
          },
        ],
      }),
    );
    expect(exchange.emitAndLogError).not.toHaveBeenCalled();
    expect(store.positions[0]).toMatchObject({ symbol: 'BTCUSDT', entryPrice: 82829.3, leverage: 3 });
  });

  it('treats an already-filled chase cancel as an already-gone order', async () => {
    const { exchange } = setup();
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: { retCode: 110001, retMsg: 'Order does not exist or is no longer open', result: {} },
      }),
    } as any;
    await expect(exchange.cancelOrdersHelper([{ id: 'order-1', symbol: 'BTCUSDT' } as Order], 'linear')).resolves.toBe(
      undefined,
    );
    expect(exchange.emitAndLogError).not.toHaveBeenCalled();
  });

  it.each([undefined, '0', 0])('keeps one-way mode for a position-topic index %s', (positionIdx) => {
    const { exchange, store } = setup();
    const { socket } = attach(exchange);
    socket.ready = true;
    socket.onMessage(
      message({
        topic: 'position',
        data: [
          {
            symbol: 'BTCUSDT',
            size: '1',
            side: 'Buy',
            avgPrice: '50000',
            markPrice: '51000',
            unrealisedPnl: '1000',
            leverage: '10',
            positionIdx,
          },
        ],
      }),
    );
    expect(store.positions[0].positionMode).toBe(PositionMode.OneWay);
  });
  it('reports an unreconciled first connection as unhealthy', () => {
    const { exchange, store } = setup();
    exchange.privateWebsocket.started = true;
    exchange.syncSocketHealthToStore();
    expect(store.socketHealth?.privateHealthy).toBe(false);
  });
  it('recovers automatically after a server-initiated native socket close', async () => {
    vi.useFakeTimers();
    const { exchange } = setup();
    const socket = exchange.privateWebsocket;
    class FakeSocket extends EventTarget {
      readyState = WebSocket.OPEN;
      send = vi.fn();
      close = vi.fn();
      ack(op: string) {
        const frame = this.send.mock.calls.map(([text]) => JSON.parse(text)).find((value) => value.op === op);
        this.dispatchEvent(message({ op, req_id: frame.req_id, success: true }));
      }
    }
    const sockets: FakeSocket[] = [];
    vi.spyOn(socket, 'createWebsocket').mockImplementation(() => {
      const ws = new FakeSocket();
      sockets.push(ws);
      return ws as any;
    });
    vi.spyOn(exchange, 'reconcilePrivateState').mockResolvedValue(true);
    const handle = vi.spyOn(socket, 'handleOrderTopic').mockImplementation(() => {});
    socket.connectAndSubscribe();
    sockets[0].dispatchEvent(new Event('open'));
    sockets[0].ack('auth');
    await vi.advanceTimersByTimeAsync(0);
    sockets[0].ack('subscribe');
    await vi.advanceTimersByTimeAsync(0);
    expect(socket.ready).toBe(true);
    sockets[0].dispatchEvent(new CloseEvent('close', { code: 1006 }));
    expect(socket.ready).toBe(false);
    await vi.advanceTimersByTimeAsync(30_001);
    expect(sockets).toHaveLength(2);
    sockets[1].dispatchEvent(new Event('open'));
    sockets[1].dispatchEvent(message({ topic: 'order', data: [{ symbol: 'BTCUSDT' }] }));
    expect(handle).not.toHaveBeenCalled();
    sockets[1].ack('auth');
    await vi.advanceTimersByTimeAsync(0);
    sockets[1].ack('subscribe');
    await vi.advanceTimersByTimeAsync(0);
    expect(socket.ready).toBe(true);
    sockets[1].dispatchEvent(message({ topic: 'order', data: [{ symbol: 'BTCUSDT' }] }));
    expect(handle).toHaveBeenCalledTimes(1);
  });
  it('waits for both acknowledgements and REST before processing events', async () => {
    const { exchange } = setup();
    const { socket, frames, ack } = attach(exchange);
    let resolve!: (value: boolean) => void;
    const snapshot = vi.spyOn(exchange, 'reconcilePrivateState').mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const handle = vi.spyOn(socket, 'handleOrderTopic').mockImplementation(() => {});
    const pending = socket.sendReconnectSequence();
    expect(frames().map(({ op }) => op)).toEqual(['auth']);
    expect(typeof frames()[0].args[1]).toBe('number');
    expect(frames()[0].args[1]).toBeGreaterThan(Date.now());
    ack('auth', true, 'wrong-session');
    await Promise.resolve();
    expect(frames().map(({ op }) => op)).toEqual(['auth']);
    ack('auth');
    await vi.waitFor(() => expect(frames().map(({ op }) => op)).toEqual(['auth', 'subscribe']));
    expect(snapshot).not.toHaveBeenCalled();
    ack('subscribe');
    await vi.waitFor(() => expect(snapshot).toHaveBeenCalledTimes(1));
    socket.onMessage(message({ topic: 'order', data: [{ symbol: 'BTCUSDT' }] }));
    expect(handle).not.toHaveBeenCalled();
    resolve(true);
    await pending;
    expect(socket.ready).toBe(true);
    socket.onMessage(message({ topic: 'order', data: [{ symbol: 'BTCUSDT' }] }));
    expect(handle).toHaveBeenCalledTimes(1);
  });
  it.each(['auth', 'subscribe'])('reconnects on %s rejection without applying events', async (op) => {
    const { exchange } = setup();
    const { socket, ack, frames } = attach(exchange);
    const reconnect = vi.spyOn(socket, 'reconnect').mockResolvedValue(undefined);
    const snapshot = vi.spyOn(exchange, 'reconcilePrivateState');
    const pending = socket.sendReconnectSequence();
    if (op === 'subscribe') {
      ack('auth');
      await vi.waitFor(() => expect(frames()).toHaveLength(2));
    }
    ack(op, false);
    await pending;
    expect(socket.ready).toBe(false);
    expect(snapshot).not.toHaveBeenCalled();
    expect(reconnect).toHaveBeenCalledTimes(1);
    expect(exchange.emitAndLogError).toHaveBeenCalled();
  });
  it('times out missing acknowledgements and reconnects', async () => {
    vi.useFakeTimers();
    const { exchange } = setup();
    const { socket } = attach(exchange);
    const reconnect = vi.spyOn(socket, 'reconnect').mockResolvedValue(undefined);
    const pending = socket.sendReconnectSequence();
    await vi.advanceTimersByTimeAsync(10_001);
    await pending;
    expect(socket.ready).toBe(false);
    expect(reconnect).toHaveBeenCalledTimes(1);
    expect(vi.mocked(exchange.emitAndLogError).mock.calls[0][0]).toMatchObject({
      message: expect.stringContaining('timed out'),
    });
  });
  it('bounds dirty snapshot retries and reconnects instead of publishing stale state', async () => {
    const { exchange } = setup();
    const { socket, ack, frames } = attach(exchange);
    const reconnect = vi.spyOn(socket, 'reconnect').mockResolvedValue(undefined);
    const snapshot = vi.spyOn(exchange, 'reconcilePrivateState').mockResolvedValue(false);
    const pending = socket.sendReconnectSequence();
    ack('auth');
    await vi.waitFor(() => expect(frames()).toHaveLength(2));
    ack('subscribe');
    await pending;
    expect(snapshot).toHaveBeenCalledTimes(3);
    expect(socket.ready).toBe(false);
    expect(reconnect).toHaveBeenCalledTimes(1);
  });
  it('does not reopen the gate after an old session snapshot resolves', async () => {
    const { exchange } = setup();
    const { socket, ack, frames } = attach(exchange);
    let resolve!: (value: boolean) => void;
    const snapshot = vi.spyOn(exchange, 'reconcilePrivateState').mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const first = socket.sendReconnectSequence();
    ack('auth');
    await vi.waitFor(() => expect(frames()).toHaveLength(2));
    ack('subscribe');
    await vi.waitFor(() => expect(snapshot).toHaveBeenCalledTimes(1));
    const second = socket.sendReconnectSequence();
    resolve(true);
    await first;
    expect(socket.ready).toBe(false);
    socket.dispose();
    await second;
  });
  it('skips base open-time snapshots and blocks creation during recovery', async () => {
    const { exchange } = setup();
    const { socket } = attach(exchange);
    const snapshot = vi.spyOn(exchange, 'fetchAndSetPrivateSnapshot');
    (socket as any).trackConnected();
    (socket as any).trackConnected();
    expect(snapshot).not.toHaveBeenCalled();
    socket.started = true;
    await expect(
      exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 1 }),
    ).rejects.toThrow('reconnecting');
  });
});
