import type { BaseExchange } from '../exchanges/base';
import type { Market, OHLCVOptions, PublicDataSource, StoreData, Ticker } from '../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { InProcessPublicDataProvider } from '../exchanges/in-process-public-data';

function createMockParent(): BaseExchange {
  const store = {
    loaded: { balance: false, orders: false, markets: false, tickers: false, positions: false },
    tickers: [] as Ticker[],
    markets: [] as Market[],
    update: vi.fn(),
    subscribe: vi.fn(() => vi.fn()),
  };

  return {
    store,
    name: 'bybit' as any,
    emitter: {
      emit: vi.fn(),
    },
  } as unknown as BaseExchange;
}

function createMockSource(opts?: {
  tickers?: Ticker[];
  markets?: Market[];
}): PublicDataSource & { _emit: (data: StoreData) => void } {
  const subscribers = new Set<(data: StoreData) => void>();
  const defaultLoaded = {
    balance: false,
    orders: false,
    markets: opts?.markets ? true : false,
    tickers: opts?.tickers ? true : false,
    positions: false,
  };

  return {
    store: {
      tickers: opts?.tickers || [],
      markets: opts?.markets || [],
      loaded: defaultLoaded,
      subscribe: vi.fn((cb) => {
        subscribers.add(cb);
        return () => subscribers.delete(cb);
      }),
    },
    listenOrderBook: vi.fn(() => vi.fn()),
    listenOHLCV: vi.fn(() => vi.fn()),
    _emit: (data: StoreData) => {
      subscribers.forEach((cb) => cb(data));
    },
  };
}

describe('InProcessPublicDataProvider', () => {
  let parent: BaseExchange;
  let source: ReturnType<typeof createMockSource>;

  beforeEach(() => {
    parent = createMockParent();
    source = createMockSource();
  });

  describe('connect', () => {
    it('should subscribe to source store', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();

      expect(source.store.subscribe).toHaveBeenCalledOnce();
      expect(provider.isConnected).toBe(true);
    });

    it('should seed tickers from source if already loaded', () => {
      const tickers = [{ symbol: 'BTCUSDT', last: 50000 }] as Ticker[];
      source = createMockSource({ tickers });

      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();

      expect(parent.store.update).toHaveBeenCalledWith(
        expect.objectContaining({
          tickers,
          loaded: expect.objectContaining({ tickers: true }),
        }),
      );
    });

    it('should seed markets from source if already loaded', () => {
      const markets = [{ symbol: 'BTCUSDT', id: 'BTCUSDT' }] as Market[];
      source = createMockSource({ markets });

      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();

      expect(parent.store.update).toHaveBeenCalledWith(
        expect.objectContaining({
          markets,
          loaded: expect.objectContaining({ markets: true }),
        }),
      );
    });

    it('should not seed empty data', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();

      expect(parent.store.update).not.toHaveBeenCalled();
    });

    it('should forward ticker updates from source', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();

      const newTickers = [{ symbol: 'BTCUSDT', last: 51000 }] as Ticker[];
      source._emit({
        tickers: newTickers,
        markets: [],
      } as unknown as StoreData);

      expect(parent.store.update).toHaveBeenCalledWith(
        expect.objectContaining({
          tickers: newTickers,
        }),
      );
    });

    it('should forward market updates from source', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();

      const newMarkets = [{ symbol: 'ETHUSDT', id: 'ETHUSDT' }] as Market[];
      source._emit({
        tickers: [],
        markets: newMarkets,
      } as unknown as StoreData);

      expect(parent.store.update).toHaveBeenCalledWith(
        expect.objectContaining({
          markets: newMarkets,
        }),
      );
    });

    it('should not forward updates after dispose', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();
      provider.dispose();

      source._emit({
        tickers: [{ symbol: 'BTCUSDT', last: 52000 }] as Ticker[],
        markets: [],
      } as unknown as StoreData);

      expect(parent.store.update).not.toHaveBeenCalled();
    });

    it('should not connect if already disposed', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.dispose();
      provider.connect();

      expect(source.store.subscribe).not.toHaveBeenCalled();
      expect(provider.isConnected).toBe(false);
    });
  });

  describe('subscribeOrderbook', () => {
    it('should delegate to source listenOrderBook', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      const callback = vi.fn();
      provider.subscribeOrderbook('BTCUSDT', callback);

      expect(source.listenOrderBook).toHaveBeenCalledWith('BTCUSDT', callback);
    });

    it('should return unsubscribe function from source', () => {
      const unsub = vi.fn();
      (source.listenOrderBook as any).mockReturnValue(unsub);

      const provider = new InProcessPublicDataProvider(parent, source);
      const result = provider.subscribeOrderbook('BTCUSDT', vi.fn());

      expect(result).toBe(unsub);
    });
  });

  describe('subscribeCandle', () => {
    it('should delegate to source listenOHLCV', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      const opts: OHLCVOptions = { symbol: 'BTCUSDT', interval: '1m' };
      const callback = vi.fn();
      provider.subscribeCandle(opts, callback);

      expect(source.listenOHLCV).toHaveBeenCalledWith(opts, callback);
    });

    it('should return unsubscribe function from source', () => {
      const unsub = vi.fn();
      (source.listenOHLCV as any).mockReturnValue(unsub);

      const provider = new InProcessPublicDataProvider(parent, source);
      const result = provider.subscribeCandle({ symbol: 'BTCUSDT', interval: '5m' }, vi.fn());

      expect(result).toBe(unsub);
    });
  });

  describe('dispose', () => {
    it('should unsubscribe from source store', () => {
      const unsub = vi.fn();
      (source.store.subscribe as any).mockReturnValue(unsub);

      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();
      provider.dispose();

      expect(unsub).toHaveBeenCalledOnce();
      expect(provider.isConnected).toBe(false);
    });

    it('should be idempotent', () => {
      const provider = new InProcessPublicDataProvider(parent, source);
      provider.connect();
      provider.dispose();
      provider.dispose(); // Should not throw

      expect(provider.isConnected).toBe(false);
    });
  });
});
