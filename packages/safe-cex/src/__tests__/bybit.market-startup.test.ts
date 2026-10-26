import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { DefaultStore } from '../store/store.base';

const createExchange = () => {
  const store = new DefaultStore();
  const exchange = new BybitExchange('bybit', { testnet: false, tuning: { consoleLogging: false } } as any, store);
  vi.spyOn(exchange as any, 'fetchFees').mockResolvedValue([]);
  return { exchange, store };
};

const marketPayload = (symbol: string, status = 'Trading') => ({
  symbol,
  baseCoin: symbol.replace(/USDT$/, ''),
  quoteCoin: 'USDT',
  settleCoin: 'USDT',
  status,
  lotSizeFilter: {
    qtyStep: '0.001',
    minOrderQty: '0.001',
    maxOrderQty: '100',
  },
  priceFilter: {
    tickSize: '0.1',
  },
  leverageFilter: {
    minLeverage: '1',
    maxLeverage: '100',
  },
  deliveryTime: '0',
});

const ok = (list: Array<Record<string, any>>) => ({ data: { retMsg: 'OK', result: { list } } });

describe('Bybit market startup', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('keeps Trading markets when optional PreLaunch markets time out', async () => {
    const { exchange } = createExchange();
    const tradingMarket = marketPayload('BTCUSDT');
    const preLaunchTimeout = new Error('Request timeout to /v5/market/instruments-info');

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') return Promise.reject(preLaunchTimeout);
      return Promise.resolve(ok([tradingMarket]));
    });
    const log = vi.spyOn(exchange, 'log');
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');

    const markets = await exchange.fetchMarkets();

    expect(markets).toHaveLength(1);
    expect(markets[0].symbol).toBe('BTCUSDT');
    expect(log).toHaveBeenCalledWith(
      'Failed to fetch Bybit PreLaunch markets, continuing with Trading markets',
      expect.any(String),
    );
    expect(emitAndLogError).not.toHaveBeenCalled();
  });

  it('falls back to cached markets when the required Trading request fails', async () => {
    const { exchange, store } = createExchange();
    const cachedMarket = marketPayload('ETHUSDT');
    store.update({ markets: [cachedMarket as any] });

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') return Promise.resolve(ok([]));
      return Promise.reject(new Error('Request timeout to /v5/market/instruments-info'));
    });
    const log = vi.spyOn(exchange, 'log');

    await expect(exchange.fetchMarkets()).resolves.toEqual([cachedMarket]);
    expect(log).toHaveBeenCalledWith('Failed to refresh Bybit markets, using cached markets', expect.any(String));
  });

  it('retries transient Trading market failures once before completing startup', async () => {
    vi.useFakeTimers();
    const { exchange } = createExchange();
    const tradingMarket = marketPayload('BTCUSDT');
    const transientError = {
      code: 'ERR_NETWORK',
      config: { method: 'get', url: ENDPOINTS.MARKETS },
      message: 'Network Error',
    };
    let tradingCalls = 0;

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') return Promise.resolve(ok([]));
      tradingCalls += 1;
      if (tradingCalls === 1) return Promise.reject(transientError);
      return Promise.resolve(ok([tradingMarket]));
    });
    const log = vi.spyOn(exchange, 'log');

    const marketsPromise = exchange.fetchMarkets();
    await vi.advanceTimersByTimeAsync(1_000);

    await expect(marketsPromise).resolves.toMatchObject([{ symbol: 'BTCUSDT' }]);
    expect(tradingCalls).toBe(2);
    expect(log).toHaveBeenCalledWith(
      'Failed to fetch Bybit Trading markets (network), retrying once in 1000ms',
      expect.any(String),
    );
  });

  it('does not retry blocked or rate-limited Trading market failures', async () => {
    const { exchange } = createExchange();
    let tradingCalls = 0;

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') return Promise.resolve(ok([]));
      tradingCalls += 1;
      return Promise.reject({
        config: { method: 'get', url: ENDPOINTS.MARKETS },
        response: { status: 429, data: { retMsg: 'too many requests' } },
      });
    });

    await expect(exchange.fetchMarkets()).rejects.toThrow('Failed to fetch Bybit Trading markets (rate-limited)');
    expect(tradingCalls).toBe(1);
  });

  it('fails startup when the required Trading response is not OK', async () => {
    const { exchange, store } = createExchange();
    store.update({ markets: [marketPayload('ETHUSDT') as any] });

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') return Promise.resolve(ok([]));
      return Promise.resolve({ data: { retMsg: 'rate limit', result: { list: [] } } });
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    const log = vi.spyOn(exchange, 'log');

    await expect(exchange.fetchMarkets()).rejects.toThrow('rate limit');
    expect(emitAndLogError).toHaveBeenCalledWith('rate limit');
    expect(log).not.toHaveBeenCalledWith('Failed to refresh Bybit markets, using cached markets', expect.any(String));
  });

  it('does not use cached markets for Trading market mapper errors', async () => {
    const { exchange, store } = createExchange();
    store.update({ markets: [marketPayload('ETHUSDT') as any] });

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') return Promise.resolve(ok([]));
      return Promise.resolve(ok([{ symbol: 'BROKENUSDT' }]));
    });
    const log = vi.spyOn(exchange, 'log');

    await expect(exchange.fetchMarkets()).rejects.toThrow();
    expect(log).not.toHaveBeenCalledWith('Failed to refresh Bybit markets, using cached markets', expect.any(String));
  });

  it('does not let slow PreLaunch markets consume the startup budget', async () => {
    vi.useFakeTimers();
    const { exchange } = createExchange();
    const tradingMarket = marketPayload('BTCUSDT');
    const preLaunchMarket = marketPayload('NEWUSDT', 'PreLaunch');

    (exchange.xhr.get as any) = vi.fn((url: string, config: any) => {
      expect(url).toBe(ENDPOINTS.MARKETS);
      if (config.params.status === 'PreLaunch') {
        return new Promise((resolve) => setTimeout(() => resolve(ok([preLaunchMarket])), 20_000));
      }
      return Promise.resolve(ok([tradingMarket]));
    });

    const marketsPromise = exchange.fetchMarkets();
    await vi.advanceTimersByTimeAsync(1_500);

    await expect(marketsPromise).resolves.toMatchObject([{ symbol: 'BTCUSDT' }]);
  });
});
