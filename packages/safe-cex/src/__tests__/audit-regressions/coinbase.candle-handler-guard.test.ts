import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Coinbase candle-handler audit regression', () => {
  it('skips products without listeners and continues dispatching the frame', () => {
    const exchange = new CoinbaseExchange(
      'coinbase' as any,
      { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
      new DefaultStore(),
    );
    const ethHandler = vi.fn();
    exchange.publicWebsocket.messageHandlers['candles.ETH-USD'] = [ethHandler];

    expect(() =>
      exchange.publicWebsocket.handleOhlcvStreamEvents({
        events: [
          {
            type: 'update',
            candles: [
              {
                product_id: 'BTC-USD',
                start: '100',
                open: '1',
                high: '2',
                low: '0.5',
                close: '1.5',
                volume: '3',
              },
              {
                product_id: 'ETH-USD',
                start: '101',
                open: '10',
                high: '20',
                low: '5',
                close: '15',
                volume: '30',
              },
            ],
          },
        ],
      } as any),
    ).not.toThrow();
    expect(ethHandler).toHaveBeenCalledWith({
      timestamp: 101,
      open: 10,
      high: 20,
      low: 5,
      close: 15,
      volume: 30,
    });
  });
});
