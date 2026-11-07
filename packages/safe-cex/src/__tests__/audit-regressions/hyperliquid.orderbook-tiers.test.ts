import { describe, expect, it, vi } from 'vitest';

import { HyperliquidPublicWebSocket } from '../reference/hyperliquid/hyperliquid.ws-public';

const createWebsocket = () => {
  const market = {
    id: 'BTC',
    symbol: 'BTC-PERP',
    precision: { price: 0.1, amount: 0.001 },
  };
  const parent = {
    isDisposed: false,
    name: 'hyperliquid',
    options: { testnet: false },
    store: {
      markets: [market],
      positions: [],
      tickersById: {},
      update: vi.fn(),
      updateTicker: vi.fn(),
    },
    emitter: { emit: vi.fn() },
    emitAndLogError: vi.fn(),
    log: vi.fn(),
    syncBalanceFromLivePositions: vi.fn(),
  };
  const websocket = new HyperliquidPublicWebSocket(parent as any);
  const send = vi.fn();
  (websocket as any).send = send;
  (websocket as any).ws = { readyState: WebSocket.OPEN, close: vi.fn() };
  return { websocket, send };
};

const depth = {
  coin: 'BTC',
  time: 1_000,
  levels: [
    [
      { px: '100', sz: '2', n: 1 },
      { px: '99', sz: '3', n: 1 },
      { px: '98', sz: '4', n: 1 },
    ],
    [
      { px: '101', sz: '5', n: 1 },
      { px: '102', sz: '6', n: 1 },
      { px: '103', sz: '7', n: 1 },
    ],
  ],
};

describe('Hyperliquid orderbook feed tiers', () => {
  it('keeps standard depth and overlays fresher BBO without stale better levels', () => {
    const { websocket, send } = createWebsocket();
    const callback = vi.fn();

    const unsubscribe = websocket.listenOrderBook('BTC-PERP', callback);
    const sent = send.mock.calls.map(([payload]) => JSON.parse(payload));
    expect(sent).toContainEqual({
      method: 'subscribe',
      subscription: { type: 'l2Book', coin: 'BTC' },
    });
    expect(sent).toContainEqual({ method: 'subscribe', subscription: { type: 'bbo', coin: 'BTC' } });

    (websocket as any).messageHandlers['l2Book.BTC'][0](depth);
    (websocket as any).messageHandlers['bbo.BTC'][0]({
      coin: 'BTC',
      time: 1_100,
      bbo: [
        { px: '99', sz: '8', n: 2 },
        { px: '102', sz: '9', n: 3 },
      ],
    });

    const combined = callback.mock.lastCall?.[0];
    expect(combined.bids).toMatchObject([
      { price: 99, amount: 8, total: 8 },
      { price: 98, amount: 4, total: 12 },
    ]);
    expect(combined.asks).toMatchObject([
      { price: 102, amount: 9, total: 9 },
      { price: 103, amount: 7, total: 16 },
    ]);

    unsubscribe();
  });

  it('unsubscribes both tiers and permits a later resubscription on the same socket', () => {
    const { websocket, send } = createWebsocket();

    const firstUnsubscribe = websocket.listenOrderBook('BTC-PERP', vi.fn());
    firstUnsubscribe();
    const secondUnsubscribe = websocket.listenOrderBook('BTC-PERP', vi.fn());

    const payloads = send.mock.calls.map(([payload]) => JSON.parse(payload));
    const count = (method: string, type: string) =>
      payloads.filter((payload) => payload.method === method && payload.subscription.type === type).length;
    expect(count('subscribe', 'l2Book')).toBe(2);
    expect(count('subscribe', 'bbo')).toBe(2);
    expect(count('unsubscribe', 'l2Book')).toBe(1);
    expect(count('unsubscribe', 'bbo')).toBe(1);

    secondUnsubscribe();
  });
});
