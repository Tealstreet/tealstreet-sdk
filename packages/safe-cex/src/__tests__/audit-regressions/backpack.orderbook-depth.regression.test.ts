import { afterEach, describe, expect, it, vi } from 'vitest';

import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { BackpackPublicWebSocket } from '../reference/backpack/backpack.ws-public';
import { DefaultStore } from '../../store/store.base';

const symbol = 'BTC_USDC_PERP';

type Snapshot = { lastUpdateId: number; bids: [string, string][]; asks: [string, string][] };
type SnapshotResponse = Snapshot | Error | Promise<Snapshot>;

const depth = (U: number, u: number, price: string) => ({
  e: 'depth' as const,
  s: symbol,
  b: [[price, '1']] as [string, string][],
  a: [] as [string, string][],
  E: 1,
  U,
  u,
});

const createWebsocket = (snapshots: SnapshotResponse[]) => {
  const store = {
    markets: [{ id: symbol, symbol }],
    tickersBySymbol: {},
    updateTicker: vi.fn(),
    update: vi.fn(),
  };
  const parent = {
    isDisposed: false,
    name: 'backpack',
    options: { testnet: false },
    store,
    emitter: { emit: vi.fn() },
    emitAndLogError: vi.fn(),
    log: vi.fn(),
    xhr: { get: vi.fn() },
  };
  parent.xhr.get.mockImplementation(() => {
    const response = snapshots.shift();
    if (response instanceof Error) return Promise.reject(response);
    if (response instanceof Promise) return response.then((data) => ({ data }));
    return Promise.resolve({ data: response });
  });

  const websocket = new BackpackPublicWebSocket(parent as any);
  websocket.lastReceived = Date.now();
  (websocket as any).ws = { readyState: WebSocket.OPEN, send: vi.fn(), close: vi.fn() };

  return { websocket, parent };
};

const flushPromises = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
};

describe('Backpack depth audit regression', () => {
  afterEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it('keeps startup market coverage on lightweight streams without opening every depth feed', async () => {
    vi.useFakeTimers();
    const exchange = new BackpackExchange(
      'backpack',
      { key: '', secret: '', testnet: false } as any,
      new DefaultStore(),
    );
    const publicWebsocket = {
      subscribeToTicker: vi.spyOn(exchange.publicWebsocket, 'subscribeToTicker').mockImplementation(() => {}),
      subscribeToTrades: vi.spyOn(exchange.publicWebsocket, 'subscribeToTrades').mockImplementation(() => {}),
      subscribeToOrderBook: vi.spyOn(exchange.publicWebsocket, 'subscribeToOrderBook').mockImplementation(() => {}),
      subscribeToBookTicker: vi.spyOn(exchange.publicWebsocket, 'subscribeToBookTicker').mockImplementation(() => {}),
    };
    const markets = [{ id: 'BTC_USDC_PERP' }, { id: 'ETH_USDC_PERP' }];

    (exchange as any).subscribeToAllMarkets(markets);
    await vi.advanceTimersByTimeAsync(2_000);

    expect(publicWebsocket.subscribeToTicker.mock.calls).toEqual([['BTC_USDC_PERP'], ['ETH_USDC_PERP']]);
    expect(publicWebsocket.subscribeToTrades.mock.calls).toEqual([['BTC_USDC_PERP'], ['ETH_USDC_PERP']]);
    expect(publicWebsocket.subscribeToBookTicker.mock.calls).toEqual([['BTC_USDC_PERP'], ['ETH_USDC_PERP']]);
    expect(publicWebsocket.subscribeToOrderBook).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it('keeps depth subscription ownership with active orderbook listeners', () => {
    const { websocket } = createWebsocket([
      { lastUpdateId: 100, bids: [['100', '1']], asks: [['101', '1']] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [['101', '1']] },
    ]);
    const send = (websocket as any).ws.send as ReturnType<typeof vi.fn>;

    const unsubscribeFirst = websocket.listenOrderBook(symbol, vi.fn());
    const unsubscribeSecond = websocket.listenOrderBook(symbol, vi.fn());
    const sentFrames = () => send.mock.calls.map(([frame]) => JSON.parse(frame));

    expect(
      sentFrames().filter((frame) => frame.method === 'SUBSCRIBE' && frame.params?.[0] === `depth.${symbol}`),
    ).toHaveLength(1);
    expect((websocket as any).topics[`depth.${symbol}`]).toBe(2);

    unsubscribeFirst();
    expect(sentFrames().filter((frame) => frame.method === 'UNSUBSCRIBE')).toHaveLength(0);
    expect((websocket as any).subscriptions.has(`depth.${symbol}`)).toBe(true);

    unsubscribeSecond();
    expect(
      sentFrames().some((frame) => frame.method === 'UNSUBSCRIBE' && frame.params?.[0] === `depth.${symbol}`),
    ).toBe(true);
    expect((websocket as any).subscriptions.has(`depth.${symbol}`)).toBe(false);
  });

  it('delivers every delta to every active orderbook handler without a shared throttle', () => {
    const { websocket } = createWebsocket([]);
    const firstHandler = vi.fn();
    const secondHandler = vi.fn();
    (websocket as any).messageHandlers[`depth.${symbol}`] = [firstHandler, secondHandler];

    for (let updateId = 101; updateId <= 104; updateId++) {
      websocket.onMessage({ data: JSON.stringify(depth(updateId, updateId, `${updateId}`)) } as MessageEvent);
    }

    expect(firstHandler).toHaveBeenCalledTimes(4);
    expect(secondHandler).toHaveBeenCalledTimes(4);
    expect(firstHandler.mock.calls.map(([update]) => update.u)).toEqual([101, 102, 103, 104]);
    expect(secondHandler.mock.calls.map(([update]) => update.u)).toEqual([101, 102, 103, 104]);
  });

  it('bridges the initial aggregate delta across the REST snapshot, then requires strict live continuity', async () => {
    const snapshot = deferred<Snapshot>();
    const { websocket } = createWebsocket([snapshot.promise]);
    const books: { bids: { price: number; amount: number }[]; asks: { price: number; amount: number }[] }[] = [];
    const callback = vi.fn((book) => {
      books.push({
        bids: book.bids.map((level: { price: number; amount: number }) => ({ ...level })),
        asks: book.asks.map((level: { price: number; amount: number }) => ({ ...level })),
      });
    });

    websocket.listenOrderBook(symbol, callback);
    websocket.onMessage({
      data: JSON.stringify({
        ...depth(99, 102, '101'),
        b: [['100', '2'], ['101', '1']],
        a: [['105', '0']],
      }),
    } as MessageEvent);
    snapshot.resolve({ lastUpdateId: 100, bids: [['100', '1']], asks: [['105', '1']] });
    await flushPromises();
    websocket.onMessage({ data: JSON.stringify(depth(103, 103, '101')) } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({ ...depth(104, 104, '101'), b: [['101', '0']] }),
    } as MessageEvent);

    expect(callback).toHaveBeenCalledTimes(3);
    expect(books[0]).toMatchObject({ bids: [{ price: 101, amount: 1 }, { price: 100, amount: 2 }], asks: [] });
    expect(books[2]).toMatchObject({ bids: [{ price: 100, amount: 2 }], asks: [] });
    expect((websocket as any).orderBookState[symbol].lastUpdateId).toBe(104);
  });

  it('resets consecutive recovery attempts after a successful resnapshot', async () => {
    const { websocket, parent } = createWebsocket([
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 102, bids: [['102', '1']], asks: [] },
      { lastUpdateId: 104, bids: [['104', '1']], asks: [] },
      { lastUpdateId: 106, bids: [['106', '1']], asks: [] },
      { lastUpdateId: 108, bids: [['108', '1']], asks: [] },
    ]);

    websocket.listenOrderBook(symbol, vi.fn());
    await flushPromises();

    for (const updateId of [102, 104, 106, 108]) {
      websocket.onMessage({ data: JSON.stringify(depth(updateId, updateId, `${updateId}`)) } as MessageEvent);
      await flushPromises();
      expect((websocket as any).orderBookState[symbol].recoveryAttempts).toBe(0);
    }

    expect(parent.xhr.get).toHaveBeenCalledTimes(5);
    expect(parent.emitAndLogError).not.toHaveBeenCalledWith(expect.stringContaining('recovery exhausted'));
  });

  it('fails closed once after three consecutive non-bridging resnapshots', async () => {
    const { websocket, parent } = createWebsocket([
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
    ]);
    const callback = vi.fn();

    websocket.listenOrderBook(symbol, callback);
    await flushPromises();
    websocket.onMessage({ data: JSON.stringify(depth(102, 102, '102')) } as MessageEvent);
    await flushPromises();

    expect(parent.xhr.get).toHaveBeenCalledTimes(4);
    expect((websocket as any).orderBookState[symbol]).toMatchObject({ isSnapshotLoaded: false, isTerminal: true });
    expect(callback.mock.calls.at(-1)?.[0]).toMatchObject({ bids: [], asks: [] });
    expect(parent.emitAndLogError).toHaveBeenCalledTimes(1);

    websocket.onMessage({ data: JSON.stringify(depth(103, 103, '103')) } as MessageEvent);
    await flushPromises();

    expect(parent.xhr.get).toHaveBeenCalledTimes(4);
    expect(parent.emitAndLogError).toHaveBeenCalledTimes(1);
  });

  it('emits one terminal error even when an orderbook consumer throws while being cleared', async () => {
    const { websocket, parent } = createWebsocket([
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
    ]);

    websocket.listenOrderBook(symbol, (book) => {
      if (book.bids.length === 0 && book.asks.length === 0) {
        throw new Error('consumer failure');
      }
    });
    await flushPromises();
    websocket.onMessage({ data: JSON.stringify(depth(102, 102, '102')) } as MessageEvent);
    await flushPromises();

    expect((websocket as any).orderBookState[symbol]).toMatchObject({ isTerminal: true, terminalErrorEmitted: true });
    expect(parent.emitAndLogError).toHaveBeenCalledTimes(1);
    expect(parent.emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('orderbook disabled'));
  });

  it('retries an initial snapshot failure, applies buffered deltas, and then clears the recovery streak', async () => {
    const { websocket, parent } = createWebsocket([
      new Error('temporary depth failure'),
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
    ]);
    const callback = vi.fn();

    websocket.listenOrderBook(symbol, callback);
    websocket.onMessage({ data: JSON.stringify(depth(101, 101, '101')) } as MessageEvent);
    await flushPromises();

    expect(parent.xhr.get).toHaveBeenCalledTimes(2);
    expect(callback).toHaveBeenCalledTimes(1);
    expect(callback.mock.calls[0][0].bids.map((level: { price: number }) => level.price)).toEqual([101, 100]);
    expect((websocket as any).orderBookState[symbol]).toMatchObject({ isSnapshotLoaded: true, recoveryAttempts: 0 });
  });

  it('coalesces gaps received while a snapshot request is in flight into one bounded recovery', async () => {
    const snapshot = deferred<Snapshot>();
    const { websocket, parent } = createWebsocket([snapshot.promise]);
    const callback = vi.fn();

    websocket.listenOrderBook(symbol, callback);
    websocket.onMessage({ data: JSON.stringify(depth(101, 101, '101')) } as MessageEvent);
    websocket.onMessage({ data: JSON.stringify(depth(102, 102, '102')) } as MessageEvent);
    websocket.onMessage({ data: JSON.stringify(depth(103, 103, '103')) } as MessageEvent);
    snapshot.resolve({ lastUpdateId: 100, bids: [['100', '1']], asks: [] });
    await flushPromises();

    expect(parent.xhr.get).toHaveBeenCalledTimes(1);
    expect(callback).toHaveBeenCalledTimes(1);
    expect((websocket as any).orderBookState[symbol]).toMatchObject({ lastUpdateId: 103, recoveryAttempts: 0 });
  });

  it('fails closed rather than retaining an unbounded delta buffer while the initial snapshot stalls', () => {
    const snapshot = deferred<Snapshot>();
    const { websocket, parent } = createWebsocket([snapshot.promise]);
    const callback = vi.fn();

    websocket.listenOrderBook(symbol, callback);
    for (let updateId = 1; updateId <= 1_001; updateId++) {
      websocket.onMessage({ data: JSON.stringify(depth(updateId, updateId, `${updateId}`)) } as MessageEvent);
    }

    expect((websocket as any).orderBookState[symbol]).toMatchObject({ isTerminal: true, updates: [] });
    expect(callback).toHaveBeenCalledTimes(1);
    expect(callback.mock.calls[0][0]).toMatchObject({ bids: [], asks: [] });
    expect(parent.emitAndLogError).toHaveBeenCalledTimes(1);
  });

  it('does not publish a late snapshot after unsubscribe or disposal', async () => {
    const unsubscribedSnapshot = deferred<Snapshot>();
    const { websocket: unsubscribedWs } = createWebsocket([unsubscribedSnapshot.promise]);
    const unsubscribedCallback = vi.fn();
    const unsubscribe = unsubscribedWs.listenOrderBook(symbol, unsubscribedCallback);
    unsubscribe();
    unsubscribedSnapshot.resolve({ lastUpdateId: 100, bids: [['100', '1']], asks: [] });
    await flushPromises();
    expect(unsubscribedCallback).not.toHaveBeenCalled();

    const disposedSnapshot = deferred<Snapshot>();
    const { websocket: disposedWs, parent } = createWebsocket([disposedSnapshot.promise]);
    const disposedCallback = vi.fn();
    disposedWs.listenOrderBook(symbol, disposedCallback);
    parent.isDisposed = true;
    disposedWs.dispose();
    disposedSnapshot.resolve({ lastUpdateId: 100, bids: [['100', '1']], asks: [] });
    await flushPromises();
    expect(disposedCallback).not.toHaveBeenCalled();
  });

  it('resets a terminal orderbook and its recovery budget on reconnect', async () => {
    const { websocket, parent } = createWebsocket([
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 100, bids: [['100', '1']], asks: [] },
      { lastUpdateId: 101, bids: [['101', '1']], asks: [] },
    ]);
    const callback = vi.fn();

    websocket.listenOrderBook(symbol, callback);
    await flushPromises();
    websocket.onMessage({ data: JSON.stringify(depth(102, 102, '102')) } as MessageEvent);
    await flushPromises();
    expect((websocket as any).orderBookState[symbol]).toMatchObject({ isTerminal: true, recoveryAttempts: 3 });

    websocket.onOpen();
    await flushPromises();

    expect((websocket as any).orderBookState[symbol]).toMatchObject({
      isTerminal: false,
      isSnapshotLoaded: true,
      recoveryAttempts: 0,
      lastUpdateId: 101,
    });
    expect(parent.xhr.get).toHaveBeenCalledTimes(5);
  });
});
