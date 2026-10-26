import type { ExchangeOptions, Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../reference/okx/okx.exchange';
import { DefaultStore } from '../../store/store.base';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const market: Market = {
  id: 'BTC-USDT-SWAP',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: {
    amount: { min: 1, max: 10 },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 1,
};

describe('OKX orderbook subscription regression', () => {
  it('preserves shared state until the final listener unsubscribes', () => {
    const store = new DefaultStore();
    store.update({
      markets: [market],
      loaded: { ...store.loaded, markets: true },
    });

    const exchange = new OKXExchange('okx', options, store);
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as unknown as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => {});

    const cleanupFirst = websocket.listenOrderBook(market.symbol, vi.fn());
    const cleanupFinal = websocket.listenOrderBook(market.symbol, vi.fn());
    const handler = `books.${market.id}`;
    websocket.messageHandlersStats[handler] = {
      lastMessageTimestamp: Date.now(),
      lastCallbacksCount: 2,
    };
    const [firstParser, finalParser] = websocket.messageHandlers[handler];
    const unsubscribePayload = JSON.stringify({
      op: 'unsubscribe',
      args: [{ channel: 'books', instId: market.id }],
    });

    expect(websocket.messageHandlers[handler]).toHaveLength(2);
    expect(websocket.topics[handler]).toEqual([{ channel: 'books', instId: market.id }]);
    expect(send).toHaveBeenCalledTimes(2);

    cleanupFirst();

    expect(websocket.messageHandlers[handler]).toEqual([finalParser]);
    expect(websocket.messageHandlers[handler]).not.toContain(firstParser);
    expect(websocket.topics[handler]).toEqual([{ channel: 'books', instId: market.id }]);
    expect(websocket.messageHandlersStats[handler]).toBeDefined();
    expect(send).toHaveBeenCalledTimes(2);
    expect(send.mock.calls.filter(([payload]) => payload === unsubscribePayload)).toHaveLength(0);

    cleanupFinal();

    expect(websocket.messageHandlers[handler]).toEqual([]);
    expect(websocket.topics[handler]).toBeUndefined();
    expect(websocket.messageHandlersStats[handler]).toBeUndefined();
    expect(send).toHaveBeenCalledTimes(3);
    expect(send.mock.calls.filter(([payload]) => payload === unsubscribePayload)).toHaveLength(1);
    expect(send).toHaveBeenNthCalledWith(3, unsubscribePayload);
  });
});
