import { describe, expect, it } from 'vitest';

import { BinancePublicWebsocket } from '../../exchanges/phemex/phemex.ws-public';
import { OrderBook } from '../../types';

const applyUpdate = (orderBook: OrderBook, update: Record<string, string[][]>) => {
  const websocket = new BinancePublicWebsocket({} as any);
  (websocket as any).processOrderBookUpdate(orderBook, update);
};

describe('Phemex orderbook audit regressions', () => {
  it('keeps inserted bid and ask levels sorted through the production delta parser', () => {
    const orderBook: OrderBook = {
      bids: [
        { price: 102, amount: 1, total: 0 },
        { price: 100, amount: 1, total: 0 },
      ],
      asks: [
        { price: 103, amount: 1, total: 0 },
        { price: 105, amount: 1, total: 0 },
      ],
    };

    applyUpdate(orderBook, { bids: [['101', '2']], asks: [['104', '3']] });

    expect(orderBook.bids.map(({ price }) => price)).toEqual([102, 101, 100]);
    expect(orderBook.asks.map(({ price }) => price)).toEqual([103, 104, 105]);
  });

  it('ignores a zero-amount delete for a level absent from the local book', () => {
    const orderBook: OrderBook = {
      bids: [{ price: 100, amount: 1, total: 0 }],
      asks: [{ price: 101, amount: 1, total: 0 }],
    };

    applyUpdate(orderBook, { bids: [['99', '0']], asks: [['102', '0']] });

    expect(orderBook).toEqual({
      bids: [{ price: 100, amount: 1, total: 0 }],
      asks: [{ price: 101, amount: 1, total: 0 }],
    });
  });
});
