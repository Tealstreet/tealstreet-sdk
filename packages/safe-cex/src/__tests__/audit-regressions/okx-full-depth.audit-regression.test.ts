import { afterEach, expect, it, vi } from 'vitest';
import type { OKXExchange } from '../reference/okx/okx.exchange';
import type { OrderBook } from '../../types';
import { OKXPublicWebsocket } from '../reference/okx/okx.ws-public';

afterEach(() => vi.unstubAllGlobals());

it('keeps valid book levels when the last trade lags the bid', () => {
  // OKX v5 books are independent of trades; no last-price filter is specified.
  vi.stubGlobal('WebSocket', { OPEN: 1 });
  const parent = {
    isDisposed: false,
    store: { loaded: { markets: true }, markets: [{ symbol: 'BTCUSDT', id: 'BTC-USDT-SWAP', precision: { amount: 0.01 } }], tickersBySymbol: { BTCUSDT: { last: 40000 } } },
  } as unknown as OKXExchange;
  const socket = new OKXPublicWebsocket(parent);
  socket.ws = { readyState: 1, send: vi.fn() } as unknown as WebSocket;
  let received: OrderBook | undefined;
  const stop = socket.listenOrderBook('BTCUSDT', (book) => { received = book; });
  socket.messageHandlers['books.BTC-USDT-SWAP'][0]({ action: 'snapshot', data: [{ bids: [['40002', '5']], asks: [['40003', '7']] }] });
  expect(received?.bids[0]).toMatchObject({ price: 40002, amount: 0.05 });
  stop();
});
