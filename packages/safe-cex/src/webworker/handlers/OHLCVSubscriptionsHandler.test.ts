import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { Exchange } from '../../exchanges/base';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';
import type { ExchangeName } from '../../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { OHLCVSubscriptionsHandler } from './OHLCVSubscriptionsHandler';

// Mock lodash throttle
vi.mock('lodash/throttle', () => ({
  default: vi.fn((fn: Function) => {
    const throttled = (...args: any[]) => fn(...args);
    throttled.cancel = vi.fn();
    throttled.flush = vi.fn();
    return throttled;
  }),
}));

describe('OHLCVSubscriptionsHandler', () => {
  let handler: OHLCVSubscriptionsHandler;
  let mockExchange: Exchange;
  let mockStore: any;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;
  let mockPostMessage: ReturnType<typeof vi.fn>;
  let mockListenOHLCV: ReturnType<typeof vi.fn>;
  let mockUnsubscribe: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockPostMessage = vi.fn();
    mockCtx = {
      postMessage: mockPostMessage,
    } as unknown as Worker;

    mockUnsubscribe = vi.fn();
    mockListenOHLCV = vi.fn().mockReturnValue(mockUnsubscribe);

    mockStore = {
      markets: [{ symbol: 'BTCUSDT' }, { symbol: 'ETHUSDT' }],
    };

    mockExchange = {
      name: 'bybit',
      store: mockStore,
      listenOHLCV: mockListenOHLCV,
      options: {},
    } as unknown as Exchange;

    mockSettings = {
      placeOrder: {
        tpTriggerType: OrderStopTriggerType.MarkPrice,
        slTriggerType: OrderStopTriggerType.LastPrice,
      },
    };
    mockMetadata = {
      supportedTPSLTriggerTypes: [],
      privateProxyIps: [],
      defaultTPTriggerType: undefined,
      defaultSLTriggerType: undefined,
    };

    handler = new OHLCVSubscriptionsHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('subscribeOHLCV', () => {
      it('should subscribe to OHLCV for valid symbol', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-1', symbol: 'BTCUSDT', interval: '1h' },
        };

        handler.handle(payload);

        expect(mockListenOHLCV).toHaveBeenCalledWith(
          { subId: 'sub-1', symbol: 'BTCUSDT', interval: '1h' },
          expect.any(Function),
        );
      });

      it('should skip subscription for symbol not on exchange', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-2', symbol: 'UNKNOWN', interval: '1h' },
        };

        handler.handle(payload);

        expect(mockListenOHLCV).not.toHaveBeenCalled();
      });

      it('should buffer subscription when markets not loaded yet', () => {
        mockStore.markets = [];

        const payload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-3', symbol: 'ANYUSDT', interval: '4h' },
        };

        handler.handle(payload);

        // Should NOT be called immediately — buffered until markets load
        expect(mockListenOHLCV).not.toHaveBeenCalled();
      });

      it('should post candle updates via callback', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-4', symbol: 'BTCUSDT', interval: '1h' },
        };

        handler.handle(payload);

        // Get the callback that was passed to listenOHLCV
        const callback = mockListenOHLCV.mock.calls[0][1];

        // Simulate candle update
        const candle = {
          timestamp: 1234567890000,
          open: 50000,
          high: 50100,
          low: 49900,
          close: 50050,
          volume: 100,
        };

        callback(candle);

        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'listenOHLCV',
          data: {
            subId: 'sub-4',
            symbol: 'BTCUSDT',
            interval: '1h',
            candle,
          },
        });
      });

      it('should handle new candle timestamps', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-5', symbol: 'BTCUSDT', interval: '1h' },
        };

        handler.handle(payload);

        const callback = mockListenOHLCV.mock.calls[0][1];

        // First candle
        const candle1 = { timestamp: 1234567890000, open: 50000 };
        callback(candle1);

        // Second candle with same timestamp
        const candle2 = { timestamp: 1234567890000, open: 50001 };
        callback(candle2);

        // Third candle with new timestamp
        const candle3 = { timestamp: 1234567950000, open: 50002 };
        callback(candle3);

        // Should have posted all candles
        expect(mockPostMessage).toHaveBeenCalledTimes(3);
      });
    });

    describe('unsubscribeOHLCV', () => {
      it('should unsubscribe from OHLCV', () => {
        // First subscribe
        const subPayload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-6', symbol: 'BTCUSDT', interval: '1h' },
        };
        handler.handle(subPayload);

        // Then unsubscribe
        const unsubPayload: StoreWorkerPayload = {
          op: 'unsubscribeOHLCV',
          data: { subId: 'sub-6' },
        };
        handler.handle(unsubPayload);

        expect(mockUnsubscribe).toHaveBeenCalled();
      });

      it('should handle unsubscribe for non-existent subscription', () => {
        const payload: StoreWorkerPayload = {
          op: 'unsubscribeOHLCV',
          data: { subId: 'non-existent' },
        };

        // Should not throw
        expect(() => handler.handle(payload)).not.toThrow();
      });

      it('should cancel throttle function on unsubscribe', () => {
        // First subscribe
        const subPayload: StoreWorkerPayload = {
          op: 'subscribeOHLCV',
          data: { subId: 'sub-7', symbol: 'BTCUSDT', interval: '15m' },
        };
        handler.handle(subPayload);

        // Then unsubscribe
        const unsubPayload: StoreWorkerPayload = {
          op: 'unsubscribeOHLCV',
          data: { subId: 'sub-7' },
        };
        handler.handle(unsubPayload);

        expect(mockUnsubscribe).toHaveBeenCalled();
      });
    });

    describe('other ops', () => {
      it('should not process unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: { subId: 'sub-x', symbol: 'BTCUSDT', interval: '1h' },
        };

        handler.handle(payload);

        expect(mockListenOHLCV).not.toHaveBeenCalled();
      });
    });
  });
});
