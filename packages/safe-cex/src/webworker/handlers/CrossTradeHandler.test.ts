import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { crossTradeManagerInstance } from '../CrossTrade/CrossTradeManager';
import { CrossTradeHandler } from './CrossTradeHandler';

// Mock CrossTradeManager
vi.mock('../CrossTrade/CrossTradeManager', () => ({
  crossTradeManagerInstance: {
    start: vi.fn(),
    stop: vi.fn(),
  },
}));

describe('CrossTradeHandler', () => {
  let handler: CrossTradeHandler;
  let mockExchange: Exchange;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;

  beforeEach(() => {
    vi.clearAllMocks();

    mockExchange = { name: 'bybit' } as Exchange;
    mockCtx = {} as Worker;
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

    handler = new CrossTradeHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('crosstrade:start', () => {
      it('should call start on manager with config', () => {
        const config = [
          {
            masterAccount: 'account1',
            copierAccount: 'account2',
            weightRatio: 0.5,
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'crosstrade:start',
          data: config,
        };

        handler.handle(payload);

        expect(vi.mocked(crossTradeManagerInstance).start).toHaveBeenCalledWith(config);
      });
    });

    describe('crosstrade:stop', () => {
      it('should call stop on manager', () => {
        const payload: StoreWorkerPayload = {
          op: 'crosstrade:stop',
        };

        handler.handle(payload);

        expect(vi.mocked(crossTradeManagerInstance).stop).toHaveBeenCalled();
      });
    });

    describe('other ops', () => {
      it('should not call manager for unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: {},
        };

        handler.handle(payload);

        expect(vi.mocked(crossTradeManagerInstance).start).not.toHaveBeenCalled();
        expect(vi.mocked(crossTradeManagerInstance).stop).not.toHaveBeenCalled();
      });
    });
  });
});
