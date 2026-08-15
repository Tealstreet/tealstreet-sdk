import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { BaseHandler } from './BaseHandler';

// Concrete implementation for testing
class TestHandler extends BaseHandler {
  handleCallCount = 0;
  lastPayload: StoreWorkerPayload | null = null;

  handle(payload: StoreWorkerPayload): void {
    this.handleCallCount++;
    this.lastPayload = payload;
  }

  buffer(payload: StoreWorkerPayload): boolean {
    return this.bufferUntilMarketsLoaded(payload);
  }

  dispose(): void {
    super.dispose();
  }

  // Expose protected members for testing
  getAccountName(): string {
    return this._accountName;
  }

  getExchangeName(): ExchangeName {
    return this._exchangeName;
  }

  getExchange(): Exchange {
    return this._exchange;
  }

  getCtx(): Worker {
    return this._ctx;
  }

  getSettings(): AppSettingsWorkerType {
    return this._settings;
  }

  getMetadata(): ExchangeMetadataWorkerType {
    return this._metadata;
  }
}

describe('BaseHandler', () => {
  const mockExchange = {
    name: 'bybit',
    options: { testnet: true, publicDataOnly: false },
    store: {
      markets: [],
      loaded: { markets: false },
      socketHealth: {
        publicInitialized: true,
        privateInitialized: false,
        publicHealthy: false,
        privateHealthy: false,
        publicCloseCode: 1006,
        publicCloseStreak: 2,
      },
    },
  } as unknown as Exchange;
  const mockCtx = { postMessage: vi.fn() } as unknown as Worker;
  const mockSettings: AppSettingsWorkerType = {
    placeOrder: {
      tpTriggerType: OrderStopTriggerType.MarkPrice,
      slTriggerType: OrderStopTriggerType.LastPrice,
    },
  };
  const mockMetadata: ExchangeMetadataWorkerType = {
    supportedTPSLTriggerTypes: [OrderStopTriggerType.MarkPrice],
    privateProxyIps: ['192.168.1.1'],
    defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
    defaultSLTriggerType: OrderStopTriggerType.LastPrice,
  };

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
    vi.mocked(mockCtx.postMessage).mockClear();
    (mockExchange.store as any).markets = [];
    (mockExchange.store as any).loaded = { markets: false };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('constructor', () => {
    it('should store accountName', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getAccountName()).toBe('my-account');
    });

    it('should store exchangeName', () => {
      const handler = new TestHandler(
        'my-account',
        'binance' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getExchangeName()).toBe('binance');
    });

    it('should store exchange reference', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getExchange()).toBe(mockExchange);
    });

    it('should store ctx (Worker)', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getCtx()).toBe(mockCtx);
    });

    it('should store settings', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getSettings()).toBe(mockSettings);
    });

    it('should store metadata', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getMetadata()).toBe(mockMetadata);
    });
  });

  describe('handle (abstract method)', () => {
    it('should be callable on concrete implementation', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      const payload: StoreWorkerPayload = { op: 'test' as any };
      handler.handle(payload);

      expect(handler.handleCallCount).toBe(1);
    });

    it('should receive payload parameter', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      const payload: StoreWorkerPayload = { op: 'test-op' as any, data: { key: 'value' } };
      handler.handle(payload);

      expect(handler.lastPayload).toBe(payload);
    });
  });

  describe('market-load buffering telemetry', () => {
    it('sends structured Sentry data when pending subscriptions time out before markets load', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'BTCUSDT' } })).toBe(true);
      expect(handler.buffer({ op: 'subscribeOHLCV' as any, data: { symbol: 'ETHUSDT', interval: '1m' } })).toBe(true);

      vi.advanceTimersByTime(30_000);

      expect(mockCtx.postMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          op: 'error',
          data: [
            expect.stringContaining('Markets still not loaded after 30s'),
            expect.objectContaining({
              sendToSentry: true,
              sentryData: expect.objectContaining({
                error: expect.objectContaining({ name: 'MarketsNotLoadedError' }),
                tags: expect.objectContaining({
                  exchange: 'bybit',
                  module: 'BaseHandler',
                  worker_handler: 'TestHandler',
                  worker_error_family: 'markets_not_loaded',
                }),
                extra: expect.objectContaining({
                  marketCount: 0,
                  loadedMarkets: false,
                  pendingCount: 2,
                  pendingAgeMs: 30_000,
                  pendingOpCounts: {
                    subscribeOrderBook: 1,
                    subscribeOHLCV: 1,
                  },
                  socketHealth: expect.objectContaining({
                    publicInitialized: true,
                    publicCloseCode: 1006,
                  }),
                }),
              }),
            }),
          ],
        }),
      );
    });

    it('suppresses repeated timeout reports during the same market-load outage', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'BTCUSDT' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      expect(handler.buffer({ op: 'subscribeOHLCV' as any, data: { symbol: 'ETHUSDT', interval: '1m' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      expect(mockCtx.postMessage).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(5 * 60_000);
      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'SOLUSDT' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      expect(mockCtx.postMessage).toHaveBeenCalledTimes(2);
      expect(mockCtx.postMessage).toHaveBeenLastCalledWith(
        expect.objectContaining({
          op: 'error',
          data: expect.arrayContaining([
            expect.any(String),
            expect.objectContaining({
              sentryData: expect.objectContaining({
                extra: expect.objectContaining({
                  suppressedPendingTimeoutReports: 1,
                }),
              }),
            }),
          ]),
        }),
      );
    });

    it('resets timeout report suppression after pending subscriptions flush', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'BTCUSDT' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      expect(handler.buffer({ op: 'subscribeOHLCV' as any, data: { symbol: 'ETHUSDT', interval: '1m' } })).toBe(true);
      (mockExchange.store as any).markets = [{ symbol: 'ETHUSDT' }];
      (mockExchange.store as any).loaded = { markets: true };
      vi.advanceTimersByTime(500);

      expect(handler.handleCallCount).toBe(1);

      (mockExchange.store as any).markets = [];
      (mockExchange.store as any).loaded = { markets: false };
      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'SOLUSDT' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      expect(mockCtx.postMessage).toHaveBeenCalledTimes(2);
      expect(mockCtx.postMessage).toHaveBeenLastCalledWith(
        expect.objectContaining({
          op: 'error',
          data: expect.arrayContaining([
            expect.any(String),
            expect.objectContaining({
              sentryData: expect.objectContaining({
                extra: expect.objectContaining({
                  suppressedPendingTimeoutReports: 0,
                }),
              }),
            }),
          ]),
        }),
      );
    });

    it('resets timeout report suppression after markets load between outages', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'BTCUSDT' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      (mockExchange.store as any).markets = [{ symbol: 'ETHUSDT' }];
      (mockExchange.store as any).loaded = { markets: true };
      expect(handler.buffer({ op: 'subscribeOHLCV' as any, data: { symbol: 'ETHUSDT', interval: '1m' } })).toBe(false);

      (mockExchange.store as any).markets = [];
      (mockExchange.store as any).loaded = { markets: false };
      expect(handler.buffer({ op: 'subscribeOrderBook' as any, data: { symbol: 'SOLUSDT' } })).toBe(true);
      vi.advanceTimersByTime(30_000);

      expect(mockCtx.postMessage).toHaveBeenCalledTimes(2);
      expect(mockCtx.postMessage).toHaveBeenLastCalledWith(
        expect.objectContaining({
          op: 'error',
          data: expect.arrayContaining([
            expect.any(String),
            expect.objectContaining({
              sentryData: expect.objectContaining({
                extra: expect.objectContaining({
                  suppressedPendingTimeoutReports: 0,
                }),
              }),
            }),
          ]),
        }),
      );
    });
  });

  describe('dispose', () => {
    it('should be callable without error', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(() => handler.dispose()).not.toThrow();
    });
  });

  describe('different exchange names', () => {
    const exchanges: ExchangeName[] = ['bybit', 'binance', 'okx', 'hyperliquid', 'blofin'] as ExchangeName[];

    exchanges.forEach((exchangeName) => {
      it(`should support ${exchangeName}`, () => {
        const handler = new TestHandler('account', exchangeName, mockExchange, mockCtx, mockSettings, mockMetadata);
        expect(handler.getExchangeName()).toBe(exchangeName);
      });
    });
  });
});
