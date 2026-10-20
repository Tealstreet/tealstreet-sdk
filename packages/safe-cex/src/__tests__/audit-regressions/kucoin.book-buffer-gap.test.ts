import type { Axios } from 'axios';

import { expect, it, vi } from 'vitest';

import { KucoinExchange } from '../../exchanges/kucoin/kucoin.exchange';
import { DefaultStore } from '../../store/store.base';

// KuCoin's documented procedure replays every cached delta after the snapshot.
// https://www.kucoin.com/docs-new/3470082w0
it('retains the entire buffered suffix when a stale snapshot leaves a gap', async () => {
  const exchange = new KucoinExchange(
    'kucoin',
    { key: '', secret: '', publicDataOnly: true, tealstreetTunnel: false },
    new DefaultStore(),
  );
  const websocket = exchange.publicWebsocket;
  vi.spyOn(websocket, 'isConnected', 'get').mockReturnValue(true);
  vi.spyOn(websocket, 'send').mockImplementation(() => {});
  const snapshots: Array<(value: unknown) => void> = [];
  const get = vi.fn().mockImplementation(() => new Promise((resolve) => snapshots.push(resolve)));
  exchange.xhr = { get } as unknown as Axios;
  const callback = vi.fn();
  const stop = websocket.listenOrderBook('BTCUSDT', callback);
  const parser = websocket.messageHandlers.orderBook_BTCUSDT[0];
  parser({ type: 'message', data: { sequence: 102, change: '99,buy,2' } });
  parser({ type: 'message', data: { sequence: 103, change: '99,buy,3' } });
  snapshots[0]({ data: { code: '200000', data: { sequence: 100, bids: [[99, 1]], asks: [] } } });
  await Promise.resolve();
  expect(get).toHaveBeenCalledTimes(2);
  expect(callback).not.toHaveBeenCalled();
  snapshots[1]({ data: { code: '200000', data: { sequence: 101, bids: [[99, 1]], asks: [] } } });
  await Promise.resolve();
  expect(get).toHaveBeenCalledTimes(2);
  expect(callback).toHaveBeenCalledOnce();
  expect(callback.mock.calls[0][0].bids).toEqual([{ price: 99, amount: 3, total: 3 }]);
  stop();
  exchange.dispose();
});

// The worker's trailing throttle retains the emitted book by reference until it sends.
it('keeps published books unchanged until recovery bridges every buffered sequence', async () => {
  const exchange = new KucoinExchange(
    'kucoin',
    { key: '', secret: '', publicDataOnly: true, tealstreetTunnel: false },
    new DefaultStore(),
  );
  const websocket = exchange.publicWebsocket;
  vi.spyOn(websocket, 'isConnected', 'get').mockReturnValue(true);
  vi.spyOn(websocket, 'send').mockImplementation(() => {});
  const snapshots: Array<(value: unknown) => void> = [];
  const get = vi.fn().mockImplementation(() => new Promise((resolve) => snapshots.push(resolve)));
  exchange.xhr = { get } as unknown as Axios;
  const callback = vi.fn();
  const stop = websocket.listenOrderBook('BTCUSDT', callback);
  const parser = websocket.messageHandlers.orderBook_BTCUSDT[0];
  const snapshot = async (index: number, sequence: number, amount: number) => {
    snapshots[index]({ data: { code: '200000', data: { sequence, bids: [[99, amount]], asks: [[101, 1]] } } });
    await Promise.resolve();
  };
  const delta = (sequence: number, amount: number) => parser({
    type: 'message', data: { sequence, change: `99,buy,${amount}` },
  });
  try {
    await snapshot(0, 100, 1);
    const published = callback.mock.calls[0][0];
    delta(101, 2);
    expect(callback.mock.calls[1][0]).toBe(published);
    expect(published.bids).toEqual([{ price: 99, amount: 2, total: 2 }]);

    delta(104, 4);
    delta(105, 5);
    delta(107, 7);
    // The first cached snapshot is too old to bridge the first buffered delta.
    await snapshot(1, 101, 90);
    expect(callback).toHaveBeenCalledTimes(2);
    expect(published.bids).toEqual([{ price: 99, amount: 2, total: 2 }]);
    // A second candidate bridges a prefix, but still has an unresolved suffix gap.
    await snapshot(2, 103, 91);
    expect(callback).toHaveBeenCalledTimes(2);
    expect(published.bids).toEqual([{ price: 99, amount: 2, total: 2 }]);

    await snapshot(3, 106, 92);
    expect(get).toHaveBeenCalledTimes(4);
    expect(callback).toHaveBeenCalledTimes(3);
    const recovered = callback.mock.calls[2][0];
    expect(recovered).not.toBe(published);
    expect(recovered.bids).toEqual([{ price: 99, amount: 7, total: 7 }]);
    delta(108, 8);
    expect(callback.mock.calls[3][0]).toBe(recovered);
    expect(recovered.bids).toEqual([{ price: 99, amount: 8, total: 8 }]);
    expect(published.bids).toEqual([{ price: 99, amount: 2, total: 2 }]);
  } finally {
    stop();
    exchange.dispose();
  }
});
