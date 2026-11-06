import { describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { BitgetUSDCExchange } from '../reference/bitget/bitgetusdc.exchange';

type BitgetVariant = {
  name: 'bitget' | 'bitgetusdc';
  expectedProductType: 'USDT-FUTURES' | 'USDC-FUTURES';
  symbols: {
    primaryTicker: string;
    secondaryTicker: string;
    added: [string, string, string];
    candle: string;
    orderBook: string;
  };
  create: () => BitgetExchange | BitgetUSDCExchange;
};

const variants: BitgetVariant[] = [
  {
    name: 'bitget',
    expectedProductType: 'USDT-FUTURES',
    symbols: {
      primaryTicker: 'BTCUSDT',
      secondaryTicker: 'ETHUSDT',
      added: ['SOLUSDT', 'XRPUSDT', 'DOGEUSDT'],
      candle: 'BTCUSDT',
      orderBook: 'ETHUSDT',
    },
    create: () =>
      new BitgetExchange(
        'bitget',
        { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase' },
        new DefaultStore(),
      ),
  },
  {
    name: 'bitgetusdc',
    expectedProductType: 'USDC-FUTURES',
    symbols: {
      primaryTicker: 'BTCPERP',
      secondaryTicker: 'ETHPERP',
      added: ['SOLPERP', 'XRPPERP', 'DOGEPERP'],
      candle: 'BTCPERP',
      orderBook: 'ETHPERP',
    },
    create: () =>
      new BitgetUSDCExchange(
        'bitgetusdc',
        { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase' },
        new DefaultStore(),
      ),
  },
];

const attachOpenSocket = (websocket: { ws?: unknown }, frames: string[]) => {
  websocket.ws = {
    readyState: WebSocket.OPEN,
    send: (frame: string) => frames.push(frame),
  } as unknown as WebSocket;
};

const parseFrames = (frames: string[]) => frames.map((frame) => JSON.parse(frame));

describe('audit regression: Bitget public websocket product types', () => {
  it.each(variants)('$name uses its parent product type for primary and secondary ticker subscriptions', ({ expectedProductType, symbols, create }) => {
    const exchange = create();
    const primaryFrames: string[] = [];
    const secondaryFrames: string[] = [];
    const primary = exchange.publicWebsocket;
    const secondary = exchange.secondsTickersWebsocket;

    primary.topics = { ticker: { [symbols.primaryTicker]: 1 } };
    secondary.topics = { ticker: { [symbols.secondaryTicker]: 1 } };
    attachOpenSocket(primary, primaryFrames);
    attachOpenSocket(secondary, secondaryFrames);
    vi.spyOn(primary, 'ping').mockImplementation(() => {});
    vi.spyOn(secondary, 'ping').mockImplementation(() => {});

    primary.onOpen();
    secondary.onOpen();
    exchange.emitter.emit('markets:changed', { added: symbols.added.map((symbol) => ({ symbol })) });

    expect(parseFrames(primaryFrames)).toEqual([
      { op: 'subscribe', args: [{ instType: expectedProductType, channel: 'ticker', instId: symbols.primaryTicker }] },
      { op: 'subscribe', args: [{ instType: expectedProductType, channel: 'ticker', instId: symbols.added[2] }] },
    ]);
    expect(parseFrames(secondaryFrames)).toEqual([
      { op: 'subscribe', args: [{ instType: expectedProductType, channel: 'ticker', instId: symbols.secondaryTicker }] },
      { op: 'subscribe', args: [{ instType: expectedProductType, channel: 'ticker', instId: symbols.added[0] }] },
    ]);
  });

  it.each(variants)('$name uses its parent product type for candle and orderbook subscribe/unsubscribe frames', ({ expectedProductType, symbols, create }) => {
    const exchange = create();
    const frames: string[] = [];
    const websocket = exchange.publicWebsocket;

    attachOpenSocket(websocket, frames);

    const stopCandle = websocket.listenOHLCV({ symbol: symbols.candle, interval: '1m' }, vi.fn());
    const stopOrderBook = websocket.listenOrderBook(symbols.orderBook, vi.fn());
    stopCandle();
    stopOrderBook();

    expect(parseFrames(frames)).toEqual([
      { op: 'subscribe', args: [{ instType: expectedProductType, channel: 'candle1m', instId: symbols.candle }] },
      { op: 'subscribe', args: [{ instType: expectedProductType, channel: 'books', instId: symbols.orderBook }] },
      { op: 'unsubscribe', args: [{ instType: expectedProductType, channel: 'candle1m', instId: symbols.candle }] },
      { op: 'unsubscribe', args: [{ instType: expectedProductType, channel: 'books', instId: symbols.orderBook }] },
    ]);
  });

  it.each(variants)(
    '$name removes final candle and orderbook topics so reconnects cannot revive them',
    ({ symbols, create }) => {
      const exchange = create();
      const frames: string[] = [];
      const websocket = exchange.publicWebsocket;

      attachOpenSocket(websocket, frames);

      const stopCandle = websocket.listenOHLCV({ symbol: symbols.candle, interval: '1m' }, vi.fn());
      const stopFirstOrderBook = websocket.listenOrderBook(symbols.orderBook, vi.fn());
      const stopSecondOrderBook = websocket.listenOrderBook(symbols.orderBook, vi.fn());

      stopCandle();
      stopFirstOrderBook();
      stopSecondOrderBook();
      stopSecondOrderBook();
      websocket.subscribe();

      expect(websocket.topics).toEqual({});
      expect(parseFrames(frames).filter((frame) => frame.op === 'unsubscribe')).toHaveLength(2);
      expect(parseFrames(frames).at(-1)).toEqual({ op: 'subscribe', args: [] });
    },
  );

  it.each(variants)(
    '$name ignores cleanup before a pending candle or orderbook subscription connects',
    ({ symbols, create }) => {
      vi.useFakeTimers();

      try {
        const exchange = create();
        const websocket = exchange.publicWebsocket;
        const stopCandle = websocket.listenOHLCV({ symbol: symbols.candle, interval: '1m' }, vi.fn());
        const stopOrderBook = websocket.listenOrderBook(symbols.orderBook, vi.fn());

        stopCandle();
        stopOrderBook();
        stopCandle();
        stopOrderBook();
        vi.runOnlyPendingTimers();

        expect(websocket.topics).toEqual({});
        // No parser survives the cleanup — the original point of this
        // regression. The keys themselves are RETAINED with empty arrays:
        // `base.ws.ts` -> `recordMessageHandlerStats` counts an absent handler
        // key as a live subscription, so deleting them made a torn-down topic
        // report as an active feed until it aged into a false stale warning.
        expect(Object.values(websocket.messageHandlers).every((handlers: unknown[]) => handlers.length === 0)).toBe(
          true,
        );
        expect(Object.keys(websocket.messageHandlers).sort()).toEqual([
          `candle_${symbols.candle}_1m`,
          `orderBook_${symbols.orderBook}`,
        ]);
      } finally {
        vi.useRealTimers();
      }
    },
  );
});
