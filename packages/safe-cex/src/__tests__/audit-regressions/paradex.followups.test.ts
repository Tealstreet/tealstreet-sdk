import { describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../../exchanges/paradex/paradex.exchange';
import { ENDPOINTS } from '../../exchanges/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';

const createExchange = () =>
  new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new DefaultStore());

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

  it('posts account max slippage to the documented profile endpoint', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ status: 200, data: {} });
    exchange.xhr = { post } as any;

    await exchange.setMaxSlippage('0.1');

    expect(post).toHaveBeenCalledWith('/account/profile/max_slippage', { max_slippage: '0.1' });
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

    await expect(exchange.fetchExecutions({ symbol: 'BTC-USD-PERP' })).resolves.toEqual([
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
});
