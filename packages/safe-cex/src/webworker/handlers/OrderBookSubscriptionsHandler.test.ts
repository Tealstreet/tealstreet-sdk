import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { Exchange } from '../../exchanges/base';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';
import type { ExchangeName } from '../../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { OrderBookSubscriptionsHandler } from './OrderBookSubscriptionsHandler';

// Mock exchangeMetadata
vi.mock('@tealstreet/safe-cex/src/exchangeMetadata', () => ({
  exchangeMetadata: {
    bybit: { orderbookTickDelay: 100 },
    binance: { orderbookTickDelay: 100 },
  },
}));

// Mock lodash throttle
vi.mock('lodash/throttle', () => ({
  default: vi.fn((fn: Function) => {
    const throttled = (...args: any[]) => fn(...args);
    throttled.cancel = vi.fn();
    return throttled;
  }),
}));

describe('OrderBookSubscriptionsHandler', () => {
  let handler: OrderBookSubscriptionsHandler;
  let mockExchange: Exchange;
  let mockStore: any;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;
  let mockPostMessage: ReturnType<typeof vi.fn>;
  let mockListenOrderBook: ReturnType<typeof vi.fn>;
  let mockUnsubscribe: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockPostMessage = vi.fn();
    mockCtx = {
      postMessage: mockPostMessage,
    } as unknown as Worker;

    mockUnsubscribe = vi.fn();
    mockListenOrderBook = vi.fn().mockReturnValue(mockUnsubscribe);

    mockStore = {
      markets: [{ symbol: 'BTCUSDT' }, { symbol: 'ETHUSDT' }],
      tickers: [{ symbol: 'BTCUSDT', ask: 50000, bid: 49999 }],
      updateTicker: vi.fn(),
    };

    mockExchange = {
      name: 'bybit',
      store: mockStore,
      listenOrderBook: mockListenOrderBook,
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

    handler = new OrderBookSubscriptionsHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('subscribeOrderBook', () => {
      it('should subscribe to orderbook for valid symbol', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-1', symbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockListenOrderBook).toHaveBeenCalledWith('BTCUSDT', expect.any(Function));
      });

      it('should skip subscription for symbol not on exchange', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-2', symbol: 'UNKNOWN' },
        };

        handler.handle(payload);

        expect(mockListenOrderBook).not.toHaveBeenCalled();
      });

      it('should buffer subscription when markets not loaded yet', () => {
        mockStore.markets = [];

        const payload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-3', symbol: 'ANYUSDT' },
        };

        handler.handle(payload);

        // Should NOT be called immediately — buffered until markets load
        expect(mockListenOrderBook).not.toHaveBeenCalled();
      });

      it('should post orderbook updates via callback', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-4', symbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        // Get the callback that was passed to listenOrderBook
        const callback = mockListenOrderBook.mock.calls[0][1];

        // Simulate orderbook update
        const orderbook = {
          asks: [{ price: 50001, amount: 1 }],
          bids: [{ price: 49999, amount: 1 }],
        };

        callback(orderbook);

        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'listenOrderBook',
          data: { subId: 'sub-4', symbol: 'BTCUSDT', orderbook },
        });
      });

      it('should update ticker BBO from orderbook', () => {
        const payload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-5', symbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        const callback = mockListenOrderBook.mock.calls[0][1];

        const orderbook = {
          asks: [{ price: 50002, amount: 1 }],
          bids: [{ price: 50000, amount: 1 }],
        };

        callback(orderbook);

        expect(mockStore.updateTicker).toHaveBeenCalledWith(mockStore.tickers[0], { ask: 50002, bid: 50000 });
      });

      it('should not update ticker if BBO unchanged', () => {
        // Set ticker to match orderbook
        mockStore.tickers = [{ symbol: 'BTCUSDT', ask: 50001, bid: 49999 }];

        const payload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-6', symbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        const callback = mockListenOrderBook.mock.calls[0][1];

        const orderbook = {
          asks: [{ price: 50001, amount: 1 }],
          bids: [{ price: 49999, amount: 1 }],
        };

        callback(orderbook);

        expect(mockStore.updateTicker).not.toHaveBeenCalled();
      });
    });

    describe('unsubscribeOrderBook', () => {
      it('should unsubscribe from orderbook', () => {
        // First subscribe
        const subPayload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-7', symbol: 'BTCUSDT' },
        };
        handler.handle(subPayload);

        // Then unsubscribe
        const unsubPayload: StoreWorkerPayload = {
          op: 'unsubscribeOrderBook',
          data: { subId: 'sub-7' },
        };
        handler.handle(unsubPayload);

        expect(mockUnsubscribe).toHaveBeenCalled();
      });

      it('should unsubscribe after a duplicate subId re-subscribes', () => {
        // Clients replay subscriptions after a reconnect, re-sending the same
        // subId. A separate refcount would climb while Set.add no-ops, leaving
        // the venue subscription stranded forever.
        const subPayload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-dup', symbol: 'BTCUSDT' },
        };
        handler.handle(subPayload);
        handler.handle(subPayload);

        expect(mockListenOrderBook).toHaveBeenCalledTimes(1);

        handler.handle({ op: 'unsubscribeOrderBook', data: { subId: 'sub-dup' } } as StoreWorkerPayload);

        expect(mockUnsubscribe).toHaveBeenCalled();
      });

      it('should keep the subscription alive while another subId remains', () => {
        handler.handle({
          op: 'subscribeOrderBook',
          data: { subId: 'sub-a', symbol: 'BTCUSDT' },
        } as StoreWorkerPayload);
        handler.handle({
          op: 'subscribeOrderBook',
          data: { subId: 'sub-b', symbol: 'BTCUSDT' },
        } as StoreWorkerPayload);

        handler.handle({ op: 'unsubscribeOrderBook', data: { subId: 'sub-a' } } as StoreWorkerPayload);
        expect(mockUnsubscribe).not.toHaveBeenCalled();

        handler.handle({ op: 'unsubscribeOrderBook', data: { subId: 'sub-b' } } as StoreWorkerPayload);
        expect(mockUnsubscribe).toHaveBeenCalled();
      });

      it('should handle unsubscribe for non-existent subscription', () => {
        const payload: StoreWorkerPayload = {
          op: 'unsubscribeOrderBook',
          data: { subId: 'non-existent' },
        };

        // Should not throw
        expect(() => handler.handle(payload)).not.toThrow();
      });

      it('should cancel throttle function on unsubscribe', () => {
        // First subscribe
        const subPayload: StoreWorkerPayload = {
          op: 'subscribeOrderBook',
          data: { subId: 'sub-8', symbol: 'BTCUSDT' },
        };
        handler.handle(subPayload);

        // Then unsubscribe
        const unsubPayload: StoreWorkerPayload = {
          op: 'unsubscribeOrderBook',
          data: { subId: 'sub-8' },
        };
        handler.handle(unsubPayload);

        // Throttle's cancel should have been called
        // (our mock throttle returns a function with cancel)
        expect(mockUnsubscribe).toHaveBeenCalled();
      });
    });

    describe('other ops', () => {
      it('should not process unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: { subId: 'sub-x', symbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockListenOrderBook).not.toHaveBeenCalled();
      });
    });
  });
});
