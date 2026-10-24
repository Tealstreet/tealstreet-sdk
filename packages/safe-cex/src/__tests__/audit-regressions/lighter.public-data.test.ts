import type { ExchangeOptions, Market, OrderBook, Ticker } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from '../../exchanges/lighter/lighter.exchange';
import { ENDPOINTS } from '../../exchanges/lighter/lighter.types';
import { DefaultStore } from '../../store/store.base';

const market: Market = {
  id: '77',
  symbol: 'BTC',
  base: 'BTC',
  quote: 'USDC',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: {
    amount: { min: 0.001, max: 1_000_000 },
    leverage: { min: 1, max: 50 },
  },
  contractSize: 1,
};

const ticker: Ticker = {
  id: market.id,
  symbol: market.symbol,
  bid: 60_000,
  ask: 60_001,
  last: 60_000,
  mark: 60_000,
  index: 60_000,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
};

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  testnet: false,
  publicDataOnly: false,
  tuning: { consoleLogging: false },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [market], tickers: [{ ...ticker }] });
  const exchange = new LighterExchange('lighter', options, store);
  const websocket = exchange.publicWebsocket;
  websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
  const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);

  return { exchange, websocket, send };
};

afterEach(() => {
  vi.useRealTimers();
});

describe('Lighter public-data audit regressions', () => {
  it('uses market-id candle/trade channels, parses live-shaped frames, and unsubscribes only after the final listener', () => {
    const { websocket, send } = createExchange();
    const candles = vi.fn();
    const firstCleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, candles);
    const finalCleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    const candleChannel = 'candle/77/1m';
    const tradeChannel = 'trade/77';

    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenNthCalledWith(1, JSON.stringify({ type: 'subscribe', channel: candleChannel }));
    expect(send).toHaveBeenNthCalledWith(2, JSON.stringify({ type: 'subscribe', channel: tradeChannel }));
    expect(websocket.topics[candleChannel]).toBe(candleChannel);
    expect(websocket.topics[tradeChannel]).toBe(tradeChannel);

    websocket.onMessage({
      data: JSON.stringify({
        channel: 'candle:77:1m',
        candles: [{ t: 1_700_000_040_000, o: 10, h: 12, l: 9, c: 11, v: 3 }],
      }),
    } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({
        channel: 'trade:77',
        trades: [{ timestamp: 1_700_000_040_500, price: '12', size: '2' }],
      }),
    } as MessageEvent);

    expect(candles).toHaveBeenNthCalledWith(1, {
      timestamp: 1_700_000_040,
      open: 10,
      high: 12,
      low: 9,
      close: 11,
      volume: 3,
    });
    expect(candles).toHaveBeenLastCalledWith({
      timestamp: 1_700_000_040,
      open: 10,
      high: 12,
      low: 9,
      close: 12,
      volume: 5,
    });

    websocket.subscribe();
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'subscribe', channel: candleChannel }));
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'subscribe', channel: tradeChannel }));

    firstCleanup();
    expect(send.mock.calls.filter(([payload]) => payload.includes('unsubscribe'))).toHaveLength(0);

    finalCleanup();
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'unsubscribe', channel: candleChannel }));
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'unsubscribe', channel: tradeChannel }));
    websocket.dispose();
  });

  it('uses exchangeStats for ticker fallback without promoting a normal fallback 403 into a websocket reconnect', async () => {
    const { exchange, websocket } = createExchange();
    const get = vi.fn().mockResolvedValue({
      code: 200,
      order_book_stats: [{ symbol: 'BTC', last_trade_price: 61_234.5, daily_base_token_volume: 42 }],
    });
    exchange.lighterApiClient.get = get;

    await (websocket as any).fetchAndApplyTickerFallback();

    expect(get).toHaveBeenCalledWith(ENDPOINTS.EXCHANGE_STATS, expect.objectContaining({ __suppressExchangeError: true }));
    expect(exchange.store.tickersBySymbol.BTC.last).toBe(61_234.5);

    const reconnect = vi.spyOn(websocket, 'reconnect');
    get.mockRejectedValueOnce({ statusCode: 403, message: 'forbidden' });
    await (websocket as any).fetchAndApplyTickerFallback();

    expect(reconnect).not.toHaveBeenCalled();
    websocket.dispose();
  });

  it('aggregates duplicate live REST order levels by exact price before sorting depth totals', async () => {
    const { exchange, websocket } = createExchange();
    const getOrderBookOrders = vi.fn().mockResolvedValue({
      marketId: 77,
      bids: [
        { price: '100', remaining_base_amount: '1.5' },
        { price: '101', remaining_base_amount: '2.5' },
        { price: '101', remaining_base_amount: '0.75' },
      ],
      asks: [
        { price: '103', remaining_base_amount: '3.5' },
        { price: '102', remaining_base_amount: '4.5' },
        { price: '102', remaining_base_amount: '1.25' },
      ],
      timestamp: 1,
    });
    exchange.orderApi.getOrderBookOrders = getOrderBookOrders;
    const orderBook: OrderBook = { bids: [], asks: [] };
    const callback = vi.fn();

    await (websocket as any).fetchAndEmitOrderBookFromRest('order_book:77', '77', orderBook, callback);

    expect(getOrderBookOrders).toHaveBeenCalledWith(77, 100, expect.objectContaining({ __suppressExchangeError: true }));
    expect(orderBook.bids.map((level) => [level.price, level.amount, level.total])).toEqual([
      [101, 3.25, 3.25],
      [100, 1.5, 4.75],
    ]);
    expect(orderBook.asks.map((level) => [level.price, level.amount, level.total])).toEqual([
      [102, 5.75, 5.75],
      [103, 3.5, 9.25],
    ]);
    expect(callback).toHaveBeenCalledWith(orderBook);
    websocket.dispose();
  });

  it('cancels an OHLCV retry when unsubscribed before the websocket connects', () => {
    vi.useFakeTimers();
    const { websocket, send } = createExchange();
    websocket.ws = { readyState: WebSocket.CONNECTING } as WebSocket;

    const cleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    cleanup();
    expect(vi.getTimerCount()).toBe(0);
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    vi.advanceTimersByTime(100);

    expect(send).not.toHaveBeenCalled();
    expect(websocket.messageHandlers['candle/77/1m']).toEqual([]);
    expect(websocket.messageHandlers['trade/77']).toEqual([]);
    expect(websocket.topics['candle/77/1m']).toBeUndefined();
    expect(websocket.topics['trade/77']).toBeUndefined();
    websocket.dispose();
  });

  it('keeps no-data recovery armed for remaining OHLCV listeners and cancels it with final cleanup', () => {
    vi.useFakeTimers();
    const { websocket, send } = createExchange();
    const reconnect = vi.spyOn(websocket, 'reconnect').mockResolvedValue(undefined);
    const candleTopic = 'candle/77/1m';
    const tradeTopic = 'trade/77';

    const firstCleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    const secondCleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    firstCleanup();

    expect(websocket.messageHandlers[candleTopic]).toHaveLength(1);
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeDefined();
    expect(websocket.topics[candleTopic]).toBe(candleTopic);
    vi.advanceTimersByTime(120_000);
    expect(reconnect).toHaveBeenCalledTimes(1);

    const finalCleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    secondCleanup();
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeDefined();
    finalCleanup();

    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeUndefined();
    expect(websocket.topics[candleTopic]).toBeUndefined();
    expect(websocket.topics[tradeTopic]).toBeUndefined();
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'unsubscribe', channel: candleTopic }));
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'unsubscribe', channel: tradeTopic }));
    vi.advanceTimersByTime(120_000);
    expect(reconnect).toHaveBeenCalledTimes(1);
    websocket.dispose();
  });

  it('rearms active OHLCV recovery after reconnect without reviving cleaned-up or disposed topics', () => {
    vi.useFakeTimers();
    const { exchange, websocket } = createExchange();
    // onOpen primes REST market stats as well as the OHLCV recovery timers.
    vi.spyOn(exchange.lighterApiClient, 'get').mockResolvedValue({ market_stats: {} });
    const candleTopic = 'candle/77/1m';
    const reconnect = vi.spyOn(websocket, 'reconnect').mockImplementation(async () => {
      websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
      websocket.onOpen();
    });

    const cleanup = websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    const firstTimeout = (websocket as any).ohlcvNoDataTimeouts[candleTopic];

    vi.advanceTimersByTime(120_000);
    expect(reconnect).toHaveBeenCalledTimes(1);
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeDefined();
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).not.toBe(firstTimeout);

    vi.advanceTimersByTime(120_000);
    expect(reconnect).toHaveBeenCalledTimes(2);

    cleanup();
    websocket.onOpen();
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeUndefined();

    websocket.listenOHLCV({ symbol: market.symbol, interval: '1m' } as any, vi.fn());
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeDefined();
    exchange.dispose();
    websocket.onOpen();
    expect((websocket as any).ohlcvNoDataTimeouts[candleTopic]).toBeUndefined();
  });

  it('rejects unsupported raw intervals before issuing a REST request', async () => {
    const { exchange, websocket } = createExchange();
    const get = vi.fn();
    exchange.lighterApiClient.get = get;

    await expect(exchange.fetchOHLCV({ symbol: market.symbol, interval: '3m' } as any)).rejects.toThrow(
      'Lighter does not support raw 3m candles',
    );
    expect(() => websocket.listenOHLCV({ symbol: market.symbol, interval: '1w' } as any, vi.fn())).toThrow(
      'Lighter does not support raw 1w candles',
    );
    expect(get).not.toHaveBeenCalled();
    websocket.dispose();
  });

  it('does not emit exchange-level Sentry errors for public candle transport retries', async () => {
    const { exchange, websocket } = createExchange();
    const get = vi.fn().mockResolvedValue({
      candles: [{ timestamp: 1_700_000_000_000, open: '10', high: '12', low: '9', close: '11', volume: '3' }],
    });
    exchange.lighterApiClient.get = get;

    await expect(exchange.fetchOHLCV({ symbol: market.symbol, interval: '1m' } as any)).resolves.toHaveLength(1);

    expect(get).toHaveBeenCalledWith(
      ENDPOINTS.CANDLES,
      expect.objectContaining({
        __suppressExchangeError: true,
      }),
    );
    websocket.dispose();
  });
});
