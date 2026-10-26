import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { UpdatePositionsHandler } from './UpdatePositionsHandler';

describe('UpdatePositionsHandler', () => {
  let handler: UpdatePositionsHandler;
  let mockExchange: Exchange;
  let mockStore: any;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;
  let mockPostMessage: ReturnType<typeof vi.fn>;
  let mockFetchPositions: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();

    mockPostMessage = vi.fn();
    mockCtx = {
      postMessage: mockPostMessage,
    } as unknown as Worker;

    mockStore = {
      positions: [],
      addOrUpdatePositions: vi.fn(),
      addOrUpdatePositionsFull: vi.fn(),
    };

    mockFetchPositions = vi.fn();

    mockExchange = {
      name: 'bybit',
      store: mockStore,
      fetchPositions: mockFetchPositions,
      isDisposed: false,
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

    handler = new UpdatePositionsHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('handle', () => {
    describe('updatePositions', () => {
      it('should fetch and update positions successfully', async () => {
        const positions = [{ symbol: 'BTCUSDT', size: 1 }];
        mockFetchPositions.mockResolvedValue(positions);

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-1' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockFetchPositions).toHaveBeenCalled();
          expect(mockStore.addOrUpdatePositionsFull).toHaveBeenCalledWith(positions);
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: { requestId: 'req-1', success: true },
          });
        });
      });

      it('should fetch positions for specific symbols', async () => {
        const positions = [{ symbol: 'BTCUSDT', size: 1 }];
        mockFetchPositions.mockResolvedValue(positions);

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-2', symbols: ['BTCUSDT', 'ETHUSDT'] },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockFetchPositions).toHaveBeenCalledWith(['BTCUSDT', 'ETHUSDT']);
        });
      });

      it('should handle empty positions result', async () => {
        mockFetchPositions.mockResolvedValue([]);

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-3' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: { requestId: 'req-3', success: true },
          });
          expect(mockStore.addOrUpdatePositions).not.toHaveBeenCalled();
          expect(mockStore.addOrUpdatePositionsFull).not.toHaveBeenCalled();
        });
      });

      it('should clear stale positions for empty full-account refreshes', async () => {
        mockStore.positions = [{ symbol: 'BTCUSDT', size: 1 }];
        mockFetchPositions.mockResolvedValue([]);

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-empty-full' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockStore.addOrUpdatePositionsFull).toHaveBeenCalledWith([]);
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: { requestId: 'req-empty-full', success: true },
          });
        });
      });

      it('should use partial updates for symbol-scoped refreshes', async () => {
        const positions = [{ symbol: 'BTCUSDT', size: 1 }];
        mockFetchPositions.mockResolvedValue(positions);

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-symbol-partial', symbols: ['BTCUSDT'] },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockStore.addOrUpdatePositions).toHaveBeenCalledWith(positions);
          expect(mockStore.addOrUpdatePositionsFull).not.toHaveBeenCalled();
        });
      });

      it('should not update if positions are equal to existing', async () => {
        const positions = [{ symbol: 'BTCUSDT', size: 1 }];
        mockStore.positions = positions;
        mockFetchPositions.mockResolvedValue(positions);

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-4' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: { requestId: 'req-4', success: true },
          });
          expect(mockStore.addOrUpdatePositions).not.toHaveBeenCalled();
        });
      });

      it('should handle fetch error and apply rate limit', async () => {
        mockFetchPositions.mockRejectedValue(new Error('API Error'));

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-5' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: {
              requestId: 'req-5',
              success: false,
              reason: 'Error: API Error',
              errorDetails: { message: 'API Error', name: 'Error', stack: expect.any(String) },
            },
          });
        });
      });

      it('should reject requests too soon after failure', async () => {
        // First request fails
        mockFetchPositions.mockRejectedValueOnce(new Error('API Error'));

        const payload1: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-6' },
        };

        handler.handle(payload1);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: {
              requestId: 'req-6',
              success: false,
              reason: 'Error: API Error',
              errorDetails: { message: 'API Error', name: 'Error', stack: expect.any(String) },
            },
          });
        });

        // Reset mock
        mockPostMessage.mockClear();
        mockFetchPositions.mockResolvedValue([{ symbol: 'BTCUSDT', size: 1 }]);

        // Second request too soon (within 5 seconds)
        const payload2: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-7' },
        };

        handler.handle(payload2);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: { requestId: 'req-7', success: false, reason: 'retry_too_soon' },
          });
          expect(mockFetchPositions).toHaveBeenCalledTimes(1); // Only first call
        });
      });

      it('should allow requests after rate limit window', async () => {
        // First request fails
        mockFetchPositions.mockRejectedValueOnce(new Error('API Error'));

        const payload1: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-8' },
        };

        handler.handle(payload1);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalled();
        });

        // Advance time past rate limit
        vi.advanceTimersByTime(5001);

        // Reset mock
        mockPostMessage.mockClear();
        mockFetchPositions.mockResolvedValue([{ symbol: 'BTCUSDT', size: 1 }]);

        // Second request after rate limit
        const payload2: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-9' },
        };

        handler.handle(payload2);

        await vi.waitFor(() => {
          expect(mockFetchPositions).toHaveBeenCalledTimes(2);
        });
      });

      it('should prevent duplicate requests for same symbol', async () => {
        // Create a promise that we control
        let resolvePositions: (value: any) => void;
        const positionsPromise = new Promise((resolve) => {
          resolvePositions = resolve;
        });
        mockFetchPositions.mockReturnValue(positionsPromise);

        // First request
        const payload1: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-10', symbols: ['BTCUSDT'] },
        };

        handler.handle(payload1);

        // Second request for same symbol
        const payload2: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-11', symbols: ['BTCUSDT'] },
        };

        handler.handle(payload2);

        // Second request should return immediately
        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'updatePositions:done',
            data: { requestId: 'req-11', success: true },
          });
        });

        // Resolve first request
        resolvePositions!([{ symbol: 'BTCUSDT', size: 1 }]);

        await vi.waitFor(() => {
          expect(mockFetchPositions).toHaveBeenCalledTimes(1); // Only called once
        });
      });

      it('should not update if exchange is disposed', async () => {
        const positions = [{ symbol: 'BTCUSDT', size: 1 }];
        mockFetchPositions.mockImplementation(async () => {
          mockExchange.isDisposed = true;
          return positions;
        });

        const payload: StoreWorkerPayload = {
          op: 'updatePositions',
          data: { requestId: 'req-12' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockFetchPositions).toHaveBeenCalled();
        });

        // Should not update store if disposed
        expect(mockStore.addOrUpdatePositions).not.toHaveBeenCalled();
        expect(mockStore.addOrUpdatePositionsFull).not.toHaveBeenCalled();
      });
    });

    describe('other ops', () => {
      it('should not process unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: { requestId: 'req-x' },
        };

        handler.handle(payload);

        expect(mockFetchPositions).not.toHaveBeenCalled();
        expect(mockPostMessage).not.toHaveBeenCalled();
      });
    });
  });
});
