import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../../exchanges/coinbase/coinbase.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Coinbase OHLCV stats cleanup audit regression', () => {
  it('cleans the USD-normalized stats keys used by USDC subscriptions', () => {
    const exchange = new CoinbaseExchange(
      'coinbase' as any,
      { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
      new DefaultStore(),
    );
    (exchange.publicWebsocket as any).ws = { readyState: WebSocket.OPEN, send: vi.fn() };

    const unsubscribe = exchange.publicWebsocket.listenOHLCV(
      { symbol: 'ETH-USDC', interval: '5m' } as any,
      vi.fn(),
    );
    exchange.publicWebsocket.messageHandlersStats['candles.ETH-USD'] = {} as any;
    exchange.publicWebsocket.messageHandlersStats['market_trades.ETH-USD'] = {} as any;

    unsubscribe();

    expect(exchange.publicWebsocket.messageHandlersStats['candles.ETH-USD']).toBeUndefined();
    expect(exchange.publicWebsocket.messageHandlersStats['market_trades.ETH-USD']).toBeUndefined();
  });
});
