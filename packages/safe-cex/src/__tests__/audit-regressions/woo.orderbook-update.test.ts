import type { OrderBook } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../reference/woo/woo.exchange';
import { DefaultStore } from '../../store/store.base';

const cloneBook = (book: OrderBook): OrderBook => ({
  bids: book.bids.map((level) => ({ ...level })),
  asks: book.asks.map((level) => ({ ...level })),
});

let exchange: WOOXExchange | undefined;

afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  vi.restoreAllMocks();
});

describe('WOO legacy orderbook delta updates', () => {
  it('replaces, removes, and inserts exact price levels while preserving sort, totals, and callback cadence', async () => {
    exchange = new WOOXExchange(
      'woo' as any,
      {
        key: 'key',
        secret: 'secret',
        applicationId: 'application-id',
        tuning: { consoleLogging: false },
      } as any,
      new DefaultStore(),
    );
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          timestamp: 100,
          bids: [
            { price: 100, quantity: 1 },
            { price: 99, quantity: 2 },
          ],
          asks: [
            { price: 101, quantity: 1.5 },
            { price: 102, quantity: 3 },
          ],
        },
      }),
    } as any;

    const websocket = exchange.publicWebsocket;
    vi.spyOn(websocket, 'isConnected', 'get').mockReturnValue(true);
    vi.spyOn(websocket, 'send').mockImplementation(() => {});
    const observedBooks: OrderBook[] = [];

    const unsubscribe = websocket.listenOrderBook('BTCUSDT', (book) => observedBooks.push(cloneBook(book)));
    await vi.waitFor(() => expect(observedBooks).toHaveLength(1));

    websocket.onMessage({
      data: JSON.stringify({
        topic: 'PERP_BTC_USDT@orderbookupdate',
        ts: 101,
        data: {
          bids: [
            [100, 3],
            [99, 0],
            [101, 2],
            [98, 4],
          ],
          asks: [
            [101, 2],
            [102, 0],
            [100.5, 5],
            [103, 1],
          ],
        },
      }),
    } as MessageEvent);

    expect(observedBooks).toHaveLength(2);
    expect(observedBooks[0]).toEqual({
      bids: [
        { price: 100, amount: 1, total: 1 },
        { price: 99, amount: 2, total: 3 },
      ],
      asks: [
        { price: 101, amount: 1.5, total: 1.5 },
        { price: 102, amount: 3, total: 4.5 },
      ],
    });
    expect(observedBooks[1]).toEqual({
      bids: [
        { price: 101, amount: 2, total: 2 },
        { price: 100, amount: 3, total: 5 },
        { price: 98, amount: 4, total: 9 },
      ],
      asks: [
        { price: 100.5, amount: 5, total: 5 },
        { price: 101, amount: 2, total: 7 },
        { price: 103, amount: 1, total: 8 },
      ],
    });

    unsubscribe();
  });
});
