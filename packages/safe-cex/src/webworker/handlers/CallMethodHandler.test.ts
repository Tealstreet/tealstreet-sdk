import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { CallMethodHandler } from './CallMethodHandler';

describe('CallMethodHandler', () => {
  let handler: CallMethodHandler;
  let mockExchange: Exchange;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;
  let mockPostMessage: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockPostMessage = vi.fn();
    mockCtx = {
      postMessage: mockPostMessage,
    } as unknown as Worker;

    mockExchange = {
      name: 'bybit',
      setLeverage: vi.fn().mockResolvedValue({ success: true }),
      updateSymbolMetadata: vi.fn().mockResolvedValue(undefined),
      changeMarginType: vi.fn().mockResolvedValue({ success: true }),
      changePositionMode: vi.fn().mockResolvedValue({ success: true }),
      fetchLastLogsFromWebsockets: vi.fn().mockReturnValue([]),
      fetchAndSetPrivateSnapshot: vi.fn().mockResolvedValue(undefined),
      reconnect: vi.fn(),
      fetchOHLCV: vi.fn().mockResolvedValue([]),
      fetchHistoricalFills: vi.fn().mockResolvedValue([]),
      fetchOrders: vi.fn().mockResolvedValue([]),
      fetchAndSetOrders: vi.fn().mockResolvedValue([]),
      removeOrders: vi.fn().mockResolvedValue(undefined),
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

    handler = new CallMethodHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('callMethod', () => {
      it('should call setLeverage and post success', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-1',
            method: 'setLeverage',
            args: ['BTCUSDT', 10],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.setLeverage).toHaveBeenCalledWith('BTCUSDT', 10);
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'callMethod:done',
            data: { requestId: 'req-1', result: { success: true }, success: true },
          });
        });
      });

      it('should call changeMarginType and post success', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-2',
            method: 'changeMarginType',
            args: ['BTCUSDT', 'CROSS'],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.changeMarginType).toHaveBeenCalledWith('BTCUSDT', 'CROSS');
          expect(mockPostMessage).toHaveBeenCalledWith(
            expect.objectContaining({
              op: 'callMethod:done',
              data: expect.objectContaining({ requestId: 'req-2', success: true }),
            }),
          );
        });
      });

      it('should call changePositionMode and post success', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-3',
            method: 'changePositionMode',
            args: ['BTCUSDT', 'Hedge'],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.changePositionMode).toHaveBeenCalledWith('BTCUSDT', 'Hedge');
        });
      });

      it('should call fetchLastLogsFromWebsockets synchronously', async () => {
        const mockLogs = [{ msg: 'test log' }];
        (mockExchange.fetchLastLogsFromWebsockets as any).mockReturnValue(mockLogs);

        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-4',
            method: 'fetchLastLogsFromWebsockets',
            args: [],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.fetchLastLogsFromWebsockets).toHaveBeenCalled();
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'callMethod:done',
            data: { requestId: 'req-4', result: mockLogs, success: true },
          });
        });
      });

      it('should call reconnect', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-5',
            method: 'reconnect',
            args: [],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.reconnect).toHaveBeenCalled();
        });
      });

      it('should call fetchOHLCV with args', async () => {
        const mockCandles = [{ time: 1234567890, open: 100 }];
        (mockExchange.fetchOHLCV as any).mockResolvedValue(mockCandles);

        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-6',
            method: 'fetchOHLCV',
            args: ['BTCUSDT', '1h', 1234567890, 100],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.fetchOHLCV).toHaveBeenCalledWith('BTCUSDT', '1h', 1234567890, 100);
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'callMethod:done',
            data: { requestId: 'req-6', result: mockCandles, success: true },
          });
        });
      });

      it('should handle method errors and post failure', async () => {
        (mockExchange.setLeverage as any).mockRejectedValue(new Error('API Error'));

        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-7',
            method: 'setLeverage',
            args: ['BTCUSDT', 10],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'callMethod:done',
            data: {
              requestId: 'req-7',
              success: false,
              result: {
                message: 'API Error',
                name: 'Error',
                stack: expect.any(String),
              },
            },
          });
        });
      });

      it.each(['COMMAND_TIMEOUT', 'ENGINE_DISPOSED', 'ENGINE_DEAD'])('retains %s command attribution', async (code) => {
        const error = Object.assign(new Error('Interrupted'), {
          code,
          command: 'setLeverage',
          timeoutMs: 120000,
          outcome: 'unknown',
          clientOrderId: 'caller',
          clientOrderIds: ['caller'],
          partialOrderIds: ['accepted'],
          partialOrderCount: 1,
          totalOrderCount: 2,
          failedOrderIndex: 1,
        });
        vi.mocked(mockExchange.setLeverage).mockRejectedValueOnce(error);
        await handler.handleCallMethod('typed', 'setLeverage', ['BTCUSDT', 10]);
        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'callMethod:done',
          data: {
            requestId: 'typed',
            success: false,
            result: {
              message: 'Interrupted',
              name: 'Error',
              stack: expect.any(String),
              code,
              command: 'setLeverage',
              timeoutMs: 120000,
              outcome: 'unknown',
              clientOrderId: 'caller',
              clientOrderIds: ['caller'],
              partialOrderIds: ['accepted'],
              partialOrderCount: 1,
              totalOrderCount: 2,
              failedOrderIndex: 1,
            },
          },
        });
      });

      it('should handle unknown method', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-8',
            method: 'unknownMethod' as any,
            args: [],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockPostMessage).toHaveBeenCalledWith({
            op: 'callMethod:done',
            data: { requestId: 'req-8', success: false },
          });
        });
      });

      it('should throttle updateSymbolMetadata calls', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-9',
            method: 'updateSymbolMetadata',
            args: ['BTCUSDT'],
          },
        };

        handler.handle(payload);

        // updateSymbolMetadata is throttled, so it might not be called immediately
        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'callMethod:done',
          data: expect.objectContaining({ requestId: 'req-9', success: true }),
        });
      });

      it('should call fetchAndSetOrders', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-10',
            method: 'fetchAndSetOrders',
            args: ['BTCUSDT'],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.fetchAndSetOrders).toHaveBeenCalledWith('BTCUSDT');
        });
      });

      it('should call removeOrders', async () => {
        const payload: StoreWorkerPayload = {
          op: 'callMethod',
          data: {
            requestId: 'req-11',
            method: 'removeOrders',
            args: [['order-1', 'order-2']],
          },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockExchange.removeOrders).toHaveBeenCalledWith(['order-1', 'order-2']);
        });
      });
    });

    describe('other ops', () => {
      it('should not process unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: {
            requestId: 'req-x',
            method: 'setLeverage',
            args: [],
          },
        };

        handler.handle(payload);

        expect(mockExchange.setLeverage).not.toHaveBeenCalled();
        expect(mockPostMessage).not.toHaveBeenCalled();
      });
    });
  });

  describe('dispose', () => {
    it('should clear throttle manager on dispose', () => {
      // Just ensure it doesn't throw
      expect(() => handler.dispose()).not.toThrow();
    });
  });
});
