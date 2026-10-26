import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { WORKER_WRITE_METHODS, workerCallClientOrderIds } from '../../exchanges/base';
import {
  decodeWorkerOrderIds,
  replacementClientOrderIdOf,
  withReplacementClientOrderId,
} from '../../exchanges/clientOrderId';
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
      it('answers the Rust native account reads a main-thread mirror forwards', async () => {
        const reads = {
          fetchPositions: vi.fn().mockResolvedValue([{ symbol: 'BTCUSDT' }]),
          fetchBalance: vi.fn().mockResolvedValue({ total: 1 }),
          fetchPositionMode: vi.fn().mockResolvedValue(true),
        };
        Object.assign(mockExchange, reads);
        for (const [method, call] of Object.entries(reads)) {
          await handler.handleCallMethod(method, method as never, method === 'fetchPositions' ? [['BTCUSDT']] : []);
          expect(call).toHaveBeenCalledOnce();
          expect(mockPostMessage).toHaveBeenLastCalledWith({
            op: 'callMethod:done',
            data: { requestId: method, result: await call.mock.results[0].value, success: true },
          });
        }
        expect(reads.fetchPositions).toHaveBeenCalledWith(['BTCUSDT']);
        for (const method of Object.keys(reads)) expect(WORKER_WRITE_METHODS.has(method)).toBe(false);
      });

      it('routes catalogue refresh to the worker engine and returns its authoritative delta', async () => {
        const delta = { added: [{ id: 'NEWUSDT' }], removed: [{ id: 'OLDUSDT' }] };
        const refresh = vi.fn().mockResolvedValue(delta);
        Object.assign(mockExchange, { refreshMarkets: refresh });
        await handler.handleCallMethod('refresh', 'refreshMarkets', []);
        expect(refresh).toHaveBeenCalledOnce();
        expect(mockPostMessage).toHaveBeenLastCalledWith({
          op: 'callMethod:done', data: { requestId: 'refresh', result: delta, success: true },
        });
        expect(WORKER_WRITE_METHODS.has('refreshMarkets')).toBe(false);
      });

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
  describe('main-thread trading methods', () => {
    it('runs a trading method on the worker exchange and returns its IDs', async () => {
      const order = { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 1, price: 2, clientOrderId: 'c-1' };
      (mockExchange as unknown as Record<string, unknown>).placeOrders = vi.fn().mockResolvedValue(['1', '2']);
      handler.handle({ op: 'callMethod', data: { requestId: 'trade-1', method: 'placeOrders', args: [[order]] } });
      await vi.waitFor(() => {
        expect(mockExchange.placeOrders).toHaveBeenCalledWith([order]);
        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'callMethod:done',
          data: { requestId: 'trade-1', result: ['1', '2'], success: true },
        });
      });
    });

    it('sends the replacement client ID of an amendment beside its IDs, surviving a JSON bridge', async () => {
      (mockExchange as unknown as Record<string, unknown>).updateOrder = vi
        .fn()
        .mockResolvedValue(withReplacementClientOrderId(['9'], 'replacement-1'));
      handler.handle({ op: 'callMethod', data: { requestId: 'amend-1', method: 'updateOrder', args: [{}] } });
      await vi.waitFor(() => expect(mockPostMessage).toHaveBeenCalled());
      const { result } = JSON.parse(JSON.stringify(mockPostMessage.mock.calls[0][0].data));
      const decoded = decodeWorkerOrderIds(result);
      expect([...decoded]).toEqual(['9']);
      expect(replacementClientOrderIdOf(decoded)).toBe('replacement-1');
    });

    it('serializes a worker trading rejection with its outcome fields', async () => {
      const failure = Object.assign(new Error('rejected by venue'), { outcome: 'rejected', clientOrderIds: ['c-1'] });
      (mockExchange as unknown as Record<string, unknown>).cancelOrders = vi.fn().mockRejectedValue(failure);
      vi.spyOn(console, 'error').mockImplementation(() => {});
      handler.handle({ op: 'callMethod', data: { requestId: 'cancel-1', method: 'cancelOrders', args: [[]] } });
      await vi.waitFor(() =>
        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'callMethod:done',
          data: {
            requestId: 'cancel-1',
            success: false,
            result: expect.objectContaining({
              message: 'rejected by venue',
              outcome: 'rejected',
              clientOrderIds: ['c-1'],
            }),
          },
        }),
      );
    });

    it('reads caller client IDs only from placement arguments', () => {
      expect(workerCallClientOrderIds('placeOrders', [[{ clientOrderId: 'a' }, {}, { clientOrderId: 'b' }]])).toEqual([
        'a',
        'b',
      ]);
      expect(workerCallClientOrderIds('placeOrderWithClientOrderId', [{ clientOrderId: 'c' }])).toEqual(['c']);
      expect(workerCallClientOrderIds('cancelOrders', [[{ clientOrderId: 'd' }]])).toBeUndefined();
      expect(workerCallClientOrderIds('placeOrder', [{}])).toBeUndefined();
      expect(workerCallClientOrderIds('placeOrders', undefined)).toBeUndefined();
    });
  });
  describe('optional engine methods', () => {
    it('runs one the worker exchange defines', async () => {
      (mockExchange as unknown as Record<string, unknown>).changeAccountTier = vi.fn().mockResolvedValue(undefined);
      handler.handle({
        op: 'callMethod',
        data: { requestId: 'tier-1', method: 'changeAccountTier', args: ['premium'] },
      });
      await vi.waitFor(() =>
        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'callMethod:done',
          data: { requestId: 'tier-1', result: undefined, success: true },
        }),
      );
      expect(
        (mockExchange as unknown as Record<string, ReturnType<typeof vi.fn>>).changeAccountTier,
      ).toHaveBeenCalledWith('premium');
    });

    it('rejects one the worker exchange lacks, with the reason', async () => {
      vi.spyOn(console, 'error').mockImplementation(() => {});
      handler.handle({ op: 'callMethod', data: { requestId: 'tier-2', method: 'fetchAccountTier', args: [] } });
      await vi.waitFor(() =>
        expect(mockPostMessage).toHaveBeenCalledWith({
          op: 'callMethod:done',
          data: {
            requestId: 'tier-2',
            success: false,
            result: expect.objectContaining({ message: 'bybit does not support fetchAccountTier' }),
          },
        }),
      );
    });
  });
});
