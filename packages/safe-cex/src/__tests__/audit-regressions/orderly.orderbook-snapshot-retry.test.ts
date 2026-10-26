import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';

describe('audit regression: Orderly orderbook snapshot recovery', () => {
  it('re-arms snapshot loading after a transient REST failure', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new DefaultStore(),
    );
    const get = vi
      .fn()
      .mockRejectedValueOnce(new Error('temporary snapshot failure'))
      .mockResolvedValueOnce({
        data: {
          data: {
            timestamp: 150,
            bids: [{ price: 100, quantity: 2 }],
            asks: [{ price: 101, quantity: 3 }],
          },
        },
      });
    exchange.xhr = { get } as any;

    const websocket = exchange.publicWebsocket;
    vi.spyOn(websocket, 'isConnected', 'get').mockReturnValue(true);
    vi.spyOn(websocket, 'send').mockImplementation(() => {});
    const emitAndLogError = vi.spyOn(websocket, 'emitAndLogError').mockImplementation(() => {});
    const callback = vi.fn();

    const unsubscribe = websocket.listenOrderBook('BTCUSDC', callback);
    const handler = websocket.messageHandlers['PERP_BTC_USDC@orderbookupdate'][0];

    handler({ ts: 100, data: { bids: [], asks: [] } });
    await vi.waitFor(() => expect(emitAndLogError).toHaveBeenCalledWith('temporary snapshot failure'));

    handler({ ts: 200, data: { bids: [[100, 4]], asks: [] } });
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(1));

    expect(callback).toHaveBeenCalledWith({
      bids: [{ price: 100, amount: 4, total: 4 }],
      asks: [{ price: 101, amount: 3, total: 3 }],
    });

    unsubscribe();
  });
});
