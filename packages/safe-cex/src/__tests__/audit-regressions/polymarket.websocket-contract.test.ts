import { afterEach, describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { PolymarketExchange } from '../reference/polymarket/polymarket.exchange';
import { PolymarketPrivateWebSocket } from '../reference/polymarket/polymarket.ws-private';
import { POLYMARKET_CONFIG } from '../reference/polymarket/polymarket.constants';

const credentials = {
  apiKey: 'api-key',
  secret: 'c2VjcmV0',
  passphrase: 'passphrase',
};

describe('Polymarket WebSocket wire contract', () => {
  afterEach(() => vi.useRealTimers());

  it('uses the official channel URLs, lowercase subscription, and raw heartbeat', () => {
    vi.useFakeTimers();
    const exchange = new PolymarketExchange(
      'polymarket',
      { publicDataOnly: true } as any,
      new DefaultStore(),
    );
    const sent: string[] = [];
    const socket = exchange.publicWebsocket;
    (socket as any).ws = {
      readyState: WebSocket.OPEN,
      send: (message: string) => sent.push(message),
      close: vi.fn(),
    };

    expect(socket.connectUrl).toBe(POLYMARKET_CONFIG.WS_MARKET);
    socket.subscribeToAsset('111');
    sent.length = 0;
    (socket as any).sendSubscription();
    socket.ping();

    expect(JSON.parse(sent[0])).toEqual({
      type: 'market',
      assets_ids: ['111'],
      custom_feature_enabled: true,
    });
    expect(sent[1]).toBe('PING');
    exchange.dispose();
  });

  it('applies price_change arrays and last_trade_price to the token ticker', () => {
    const store = new DefaultStore();
    store.update({ tickers: [{ id: '111', symbol: 'v2-adapter-Yes', last: 0 } as any] });
    const exchange = new PolymarketExchange(
      'polymarket',
      { publicDataOnly: true } as any,
      store,
    );
    (exchange as any).tokenToSymbol.set('111', 'v2-adapter-Yes');

    (exchange.publicWebsocket as any).handlePriceChange({
      event_type: 'price_change',
      price_changes: [
        {
          asset_id: '111',
          price: '0.50',
          size: '2',
          side: 'BUY',
          best_bid: '0.48',
          best_ask: '0.52',
        },
      ],
    });
    (exchange.publicWebsocket as any).handleLastTradePrice({
      event_type: 'last_trade_price',
      asset_id: '111',
      price: '0.51',
    });

    expect(store.tickers[0]).toMatchObject({
      bid: 0.48,
      ask: 0.52,
      mark: 0.5,
      index: 0.5,
      last: 0.51,
    });
    exchange.dispose();
  });

  it('authenticates the user channel with the official lowercase payload', () => {
    const exchange = new PolymarketExchange(
      'polymarket',
      { publicDataOnly: true } as any,
      new DefaultStore(),
    );
    const sent: string[] = [];
    const socket = new PolymarketPrivateWebSocket(exchange, () => credentials);
    (socket as any).ws = {
      readyState: WebSocket.OPEN,
      send: (message: string) => sent.push(message),
      close: vi.fn(),
    };

    expect(socket.connectUrl).toBe(POLYMARKET_CONFIG.WS_USER);
    (socket as any).authenticate();

    expect(JSON.parse(sent[0])).toEqual({
      type: 'user',
      auth: {
        apiKey: 'api-key',
        secret: 'c2VjcmV0',
        passphrase: 'passphrase',
      },
    });
    socket.dispose();
    exchange.dispose();
  });
});
