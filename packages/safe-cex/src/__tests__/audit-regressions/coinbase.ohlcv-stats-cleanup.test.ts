import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';

describe('Coinbase OHLCV stats cleanup audit regression', () => {
  it('cleans the USD-normalized stats keys used by USDC subscriptions', () => {
    const exchange = new CoinbaseExchange(
      'coinbase' as any,
      { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
      new DefaultStore(),
    );
    const send = vi.fn();
    (exchange.publicWebsocket as any).ws = { readyState: WebSocket.OPEN, send };

    const unsubscribe = exchange.publicWebsocket.listenOHLCV(
      { symbol: 'ETH-USDC', interval: '5m' } as any,
      vi.fn(),
    );
    exchange.publicWebsocket.messageHandlersStats['candles.ETH-USD'] = {} as any;
    exchange.publicWebsocket.messageHandlersStats['market_trades.ETH-USD'] = {} as any;

    unsubscribe();

    expect(exchange.publicWebsocket.messageHandlersStats['candles.ETH-USD']).toBeUndefined();
    expect(exchange.publicWebsocket.messageHandlersStats['market_trades.ETH-USD']).toBeUndefined();
    expect(send.mock.calls.map(([payload]) => JSON.parse(String(payload)))).toEqual([
      { type: 'subscribe', product_ids: ['ETH-USD'], channel: 'candles' },
      { type: 'subscribe', product_ids: ['ETH-USD'], channel: 'market_trades' },
      { type: 'unsubscribe', product_ids: ['ETH-USD'], channel: 'candles' },
      { type: 'unsubscribe', product_ids: ['ETH-USD'], channel: 'market_trades' },
    ]);
  });

  it('requests REST candles with the Coinbase USD-equivalent product id', async () => {
    const exchange = new CoinbaseExchange(
      'coinbase' as any,
      { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
      new DefaultStore(),
    );
    const get = vi.fn().mockResolvedValue({
      data: {
        candles: [
          {
            start: '1700000000',
            low: '99',
            high: '101',
            open: '100',
            close: '100.5',
            volume: '10',
          },
        ],
      },
    });
    exchange.xhr = { get } as any;

    await exchange.fetchOHLCV({ symbol: 'BTC-USDC', interval: '1m', limit: 1 } as any);

    expect(get.mock.calls[0]?.[0]).toBe(ENDPOINTS.KLINE);
    expect(get.mock.calls[0]?.[1].params.product_id).toBe('BTC-USD');
  });
});
