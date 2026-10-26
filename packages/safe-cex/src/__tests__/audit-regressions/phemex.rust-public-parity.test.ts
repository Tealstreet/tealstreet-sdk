import { describe, expect, it, vi } from 'vitest';

import type { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { BinancePublicWebsocket } from '../reference/phemex/phemex.ws-public';

// Official USDⓈ-M docs specify [symbol, false, 0] for full depth at 20 ms.
// Snapshot sequence belongs to the outer frame, alongside type and depth.
const websocket = (get = vi.fn()) => {
  const parent = {
    emitter: { emit: vi.fn() },
    isDisposed: false,
    log: vi.fn(),
    options: { testnet: false },
    store: {},
    xhr: { get },
  } as unknown as PhemexExchange;
  const socket = new BinancePublicWebsocket(parent);
  vi.spyOn(socket, 'isConnected', 'get').mockReturnValue(true);
  const send = vi.spyOn(socket, 'send').mockImplementation(() => undefined);
  return { socket, send };
};
const frame = (
  socket: BinancePublicWebsocket,
  sequence: number,
  type: 'snapshot' | 'incremental',
  bids: string[][],
  asks: string[][],
) => socket.onMessage({
  data: JSON.stringify({ symbol: 'BTCUSDT', sequence, type, depth: 0, orderbook_p: { bids, asks } }),
} as MessageEvent);
const snapshot = (sequence: number) => ({
  data: { result: { sequence, orderbook_p: { bids: [['100', '1'], ['99', '1']], asks: [['101', '1']] } } },
});

describe('Phemex Rust public replay oracle parity', () => {
  it('requests full depth on initial subscription and reconnect', () => {
    const { socket, send } = websocket();
    const stop = socket.listenOrderBook('BTCUSDT', vi.fn());
    expect(JSON.parse(send.mock.calls[0][0])).toMatchObject({ method: 'orderbook_p.subscribe', params: ['BTCUSDT', false, 0] });

    send.mockClear();
    socket.subscribe();
    expect(JSON.parse(send.mock.calls[0][0])).toMatchObject({ method: 'orderbook_p.subscribe', params: ['BTCUSDT', false, 0] });
    stop();
  });

  it('uses outer sequence to discard stale buffered updates and replaces later full snapshots', async () => {
    let resolve!: (value: ReturnType<typeof snapshot>) => void;
    const pending = new Promise<ReturnType<typeof snapshot>>((done) => { resolve = done; });
    const get = vi.fn().mockReturnValue(pending);
    const { socket } = websocket(get);
    const callback = vi.fn();
    const stop = socket.listenOrderBook('BTCUSDT', callback);

    frame(socket, 100, 'incremental', [['100', '999']], []);
    frame(socket, 102, 'incremental', [['100', '2']], []);
    expect(callback).not.toHaveBeenCalled();
    expect(get).toHaveBeenCalledWith('/md/v2/fullbook', { params: { symbol: 'BTCUSDT', limit: 1000 } });
    resolve(snapshot(101));
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));
    expect(callback.mock.calls[0][0].bids.map(({ price, amount }: { price: number; amount: number }) => [price, amount])).toEqual([[100, 2], [99, 1]]);

    frame(socket, 103, 'snapshot', [['98', '3']], [['102', '4']]);
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callback.mock.calls[1][0].bids).toEqual([{ price: 98, amount: 3, total: 3 }]);
    expect(callback.mock.calls[1][0].asks).toEqual([{ price: 102, amount: 4, total: 4 }]);
    stop();
  });

  it('keeps buffered full-snapshot replacement semantics while REST is pending', async () => {
    let resolve!: (value: ReturnType<typeof snapshot>) => void;
    const get = vi.fn().mockReturnValue(new Promise<ReturnType<typeof snapshot>>((done) => { resolve = done; }));
    const { socket } = websocket(get);
    const callback = vi.fn();
    const stop = socket.listenOrderBook('BTCUSDT', callback);

    frame(socket, 102, 'incremental', [['100.5', '9']], []);
    frame(socket, 103, 'snapshot', [['100.7', '3']], [['101.7', '4']]);
    frame(socket, 104, 'incremental', [['100.7', '5']], [['101.7', '0'], ['102', '2']]);
    resolve(snapshot(101));
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));
    expect(callback.mock.calls[0][0].bids.map(({ price, amount }: { price: number; amount: number }) => [price, amount])).toEqual([[100.7, 5]]);
    expect(callback.mock.calls[0][0].asks.map(({ price, amount }: { price: number; amount: number }) => [price, amount])).toEqual([[102, 2]]);
    stop();
  });
});
