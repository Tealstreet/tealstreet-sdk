import type { Axios } from 'axios';

import { expect, it, vi } from 'vitest';

import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
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
