import { afterEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';

const createExchange = () =>
  new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new DefaultStore());

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('audit regression: Paradex current REST and lifecycle contracts', () => {
  it('allows the legacy TradingView route without a JWT in public-data-only mode', async () => {
    const exchange = createExchange();
    const adapter = vi.fn(async (config) => ({
      data: { s: 'ok', t: [], o: [], h: [], l: [], c: [], v: [] },
      status: 200,
      statusText: 'OK',
      headers: {},
      config,
    }));

    await expect(exchange.xhr.get(ENDPOINTS.TRADINGVIEW_HISTORY, { adapter })).resolves.toMatchObject({ status: 200 });
    expect(adapter).toHaveBeenCalledOnce();
  });

  it('uses and parses the documented klines endpoint for supported intervals', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: { results: [[1_750_000_000_000, 100, 110, 90, 105, 42]] },
    });
    exchange.xhr = { get } as any;

    await expect(
      exchange.fetchOHLCV({ symbol: 'BTC-USD-PERP', interval: '1h', from: 1_749_996_400_000, to: 1_750_000_000_000 }),
    ).resolves.toEqual([{ timestamp: 1_750_000_000_000, open: 100, high: 110, low: 90, close: 105, volume: 42 }]);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.OHLCV, {
      params: {
        symbol: 'BTC-USD-PERP',
        resolution: '60',
        start_at: 1_749_996_400_000,
        end_at: 1_750_000_000_000,
        price_kind: 'last',
      },
    });
  });

  it('retains TradingView only for wider intervals unsupported by klines', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: { s: 'ok', t: [1_750_000_000], o: ['100'], h: ['110'], l: ['90'], c: ['105'], v: ['42'] },
    });
    exchange.xhr = { get } as any;

    await expect(
      exchange.fetchOHLCV({ symbol: 'BTC-USD-PERP', interval: '4h', from: 1_749_985_600_000, to: 1_750_000_000_000 }),
    ).resolves.toEqual([{ timestamp: 1_750_000_000_000, open: 100, high: 110, low: 90, close: 105, volume: 42 }]);
    expect(get).toHaveBeenCalledWith(
      ENDPOINTS.TRADINGVIEW_HISTORY,
      expect.objectContaining({ params: expect.objectContaining({ resolution: '240' }) }),
    );
  });

  it('posts account max slippage to the documented per-market profile endpoint', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ status: 200, data: {} });
    exchange.xhr = { post } as any;

    await exchange.setMaxSlippage('0.1', 'BTC-USD-PERP');

    expect(post).toHaveBeenCalledWith('/account/profile/market_max_slippage/BTC-USD-PERP', { max_slippage: '0.1' });
  });

  it('uses order history, not open orders, when fills are unavailable', async () => {
    const exchange = createExchange();
    Object.defineProperty(exchange, 'isPublicDataOnly', { value: false });
    exchange.store.update({
      markets: [
        {
          id: 'BTC-USD-PERP',
          symbol: 'BTC-USD-PERP',
          base: 'BTC',
          quote: 'USD',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
        },
      ],
    });
    (exchange as any).jwtManager = {};
    const get = vi.fn(async (endpoint: string) => {
      if (endpoint === ENDPOINTS.FILLS) return { data: { results: [] } };
      return {
        data: {
          results: [
            {
              order_id: 'order-1',
              market: 'BTC-USD-PERP',
              side: 'BUY',
              price: '100',
              size: '2',
              remaining_size: '0.5',
              created_at: 1_750_000_000_000,
            },
          ],
        },
      };
    });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchHistoricalFills({ symbol: 'BTC-USD-PERP' })).resolves.toEqual([
      expect.objectContaining({ orderId: 'order-1', price: 100, qty: 1.5, timestamp: 1_750_000_000 }),
    ]);
    expect(get.mock.calls.map(([endpoint]) => endpoint)).toEqual([ENDPOINTS.FILLS, ENDPOINTS.ORDERS_HISTORY]);
  });

  it.each(['publicWebsocket', 'privateWebsocket'] as const)('$websocket delegates disposal to BaseWebSocket', (key) => {
    const exchange = createExchange();
    const socket = exchange[key];
    const close = vi.fn();
    const terminate = vi.fn();
    socket.ws = { close, terminate } as any;

    socket.dispose();

    expect(close).toHaveBeenCalledOnce();
    expect(terminate).toHaveBeenCalledOnce();
    expect((socket as any).forceClosed).toBe(true);
    expect(socket.ws).toBeUndefined();
  });

  it('keeps a shared OHLCV listener alive when an older same-key listener unsubscribes', () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    vi.spyOn(exchange, 'fetchOHLCV').mockResolvedValue([]);
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    websocket.ws = { readyState: WebSocket.OPEN, send: vi.fn() } as any;

    const first = vi.fn();
    const second = vi.fn();
    const firstCleanup = websocket.listenOHLCV({ symbol: 'BTC-USD-PERP', interval: '1m' }, first);
    const secondCleanup = websocket.listenOHLCV({ symbol: 'BTC-USD-PERP', interval: '1m' }, second);

    firstCleanup();
    (websocket as any).handleTradesUpdate({
      channel: 'trades.BTC-USD-PERP',
      data: {
        market: 'BTC-USD-PERP',
        price: '101',
        size: '2',
        created_at: 1_700_000_060_500,
      },
    });

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith({
      timestamp: 1_700_000_040_000,
      open: 101,
      high: 101,
      low: 101,
      close: 101,
      volume: 2,
    });

    secondCleanup();
    expect((websocket as any).ohlcvCallbacks.has('BTC-USD-PERP:1m')).toBe(false);
    exchange.dispose();
  });

  it('processes batched trade updates using Paradex trade timestamps', () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_800_000_000_000);
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    vi.spyOn(exchange, 'fetchOHLCV').mockResolvedValue([]);
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    const candles = vi.fn();
    const cleanup = websocket.listenOHLCV({ symbol: 'BTC-USD-PERP', interval: '1m' }, candles);

    (websocket as any).handleTradesUpdate({
      channel: 'trades.BTC-USD-PERP',
      data: [
        { market: 'BTC-USD-PERP', price: '100', size: '1', created_at: 1_700_000_060_500 },
        { market: 'BTC-USD-PERP', price: '105', size: '2', created_at: 1_700_000_061_500 },
      ],
    });

    expect(candles).toHaveBeenNthCalledWith(1, {
      timestamp: 1_700_000_040_000,
      open: 100,
      high: 100,
      low: 100,
      close: 100,
      volume: 1,
    });
    expect(candles).toHaveBeenNthCalledWith(2, {
      timestamp: 1_700_000_040_000,
      open: 100,
      high: 105,
      low: 100,
      close: 105,
      volume: 3,
    });

    cleanup();
    exchange.dispose();
  });

  it('retries failed Paradex trade channel subscriptions while the symbol is still desired', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    const send = vi.fn();
    websocket.ws = { readyState: WebSocket.OPEN, send } as any;

    websocket.subscribeToTrades('BTC-USD-PERP');
    expect(send).toHaveBeenCalledWith(
      JSON.stringify({
        id: 0,
        jsonrpc: '2.0',
        method: 'subscribe',
        params: { channel: 'trades.BTC-USD-PERP' },
      }),
    );

    websocket.onMessage({
      data: JSON.stringify({
        jsonrpc: '2.0',
        id: 0,
        error: { message: 'temporary subscription failure' },
      }),
    } as MessageEvent);

    await vi.advanceTimersByTimeAsync(1_000);

    expect(send).toHaveBeenLastCalledWith(
      JSON.stringify({
        id: 1,
        jsonrpc: '2.0',
        method: 'subscribe',
        params: { channel: 'trades.BTC-USD-PERP' },
      }),
    );

    exchange.dispose();
  });
});
