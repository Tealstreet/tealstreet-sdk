import { describe, expect, it, vi } from 'vitest';

import { BinancePublicWebsocket } from '../reference/phemex/phemex.ws-public';
import { LogSeverity, OrderBook } from '../../types';

const applyUpdate = (orderBook: OrderBook, update: Record<string, string[][]>) => {
  const websocket = new BinancePublicWebsocket({} as any);
  (websocket as any).processOrderBookUpdate(orderBook, update);
};

const createOrderbookWebsocket = (get: ReturnType<typeof vi.fn>) => {
  const parent = {
    emitter: { emit: vi.fn() },
    isDisposed: false,
    log: vi.fn(),
    options: { testnet: false },
    store: {},
    xhr: { get },
  } as any;
  const websocket = new BinancePublicWebsocket(parent);
  vi.spyOn(websocket, 'isConnected', 'get').mockReturnValue(true);
  vi.spyOn(websocket, 'send').mockImplementation(() => undefined);

  return { parent, websocket };
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

  it('re-arms snapshot loading after a transient REST failure', async () => {
    const get = vi
      .fn()
      .mockRejectedValueOnce(new Error('[phemex] Request setup error for /md/v2/fullbook: WSOverHTTP request timed out'))
      .mockResolvedValueOnce({
        data: {
          result: {
            orderbook_p: {
              bids: [['100', '1']],
              asks: [['101', '2']],
            },
            sequence: 10,
          },
        },
      });
    const { parent, websocket } = createOrderbookWebsocket(get);
    const callback = vi.fn();

    const unsubscribe = websocket.listenOrderBook('NEIROUSDT', callback);
    const handler = websocket.messageHandlers['orderbook_p.NEIROUSDT'][0];

    handler({ sequence: 11, bids: [['100.5', '3']], asks: [] });
    await vi.waitFor(() =>
      expect(parent.log).toHaveBeenCalledWith(
        expect.stringContaining('Orderbook snapshot request failed for NEIROUSDT'),
        LogSeverity.Warning,
      ),
    );
    expect(callback).not.toHaveBeenCalled();

    handler({ sequence: 12, bids: [['100.75', '4']], asks: [] });
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));

    expect(callback.mock.calls[0][0].bids.map((level: { price: number }) => level.price)).toEqual([100.75, 100]);

    unsubscribe();
  });
});
