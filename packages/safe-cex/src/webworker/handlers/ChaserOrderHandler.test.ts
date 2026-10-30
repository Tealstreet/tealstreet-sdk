import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide, OrderStopTriggerType } from '../../types';
import { ChaserOrderInstance } from '../chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance } from '../chaserOrder/ChaserOrderManager';
import { ChaserOrderHandler } from './ChaserOrderHandler';

// Mock ChaserOrderInstance and chaserOrderManagerInstance
vi.mock('../chaserOrder/ChaserOrderInstance', () => ({
  ChaserOrderInstance: vi.fn().mockImplementation((exchange, options) => ({
    exchange,
    options,
  })),
}));

vi.mock('../chaserOrder/ChaserOrderManager', () => ({
  chaserOrderManagerInstance: {
    add: vi.fn(),
    remove: vi.fn(),
  },
}));

describe('ChaserOrderHandler', () => {
  let handler: ChaserOrderHandler;
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

    handler = new ChaserOrderHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('chaser:start', () => {
      it('should create ChaserOrderInstance and add to manager', () => {
        const payload: StoreWorkerPayload = {
          op: 'chaser:start',
          data: {
            symbol: 'BTCUSDT',
            side: OrderSide.Buy,
            size: 1.0,
            maxAttempts: 10,
            reduceOnly: false,
            origin: 'web',
          },
        };

        handler.handle(payload);

        expect(ChaserOrderInstance).toHaveBeenCalledWith(
          mockExchange,
          expect.objectContaining({
            symbol: 'BTCUSDT',
            side: OrderSide.Buy,
            size: 1.0,
            accountName: 'test-account',
          }),
        );
        expect(chaserOrderManagerInstance.add).toHaveBeenCalled();
      });

      it('should use provided accountName if available', () => {
        const payload: StoreWorkerPayload = {
          op: 'chaser:start',
          data: {
            symbol: 'BTCUSDT',
            side: OrderSide.Buy,
            size: 1.0,
            accountName: 'custom-account',
          },
        };

        handler.handle(payload);

        expect(ChaserOrderInstance).toHaveBeenCalledWith(
          mockExchange,
          expect.objectContaining({
            accountName: 'custom-account',
          }),
        );
      });

      it('should fallback to handler accountName if not provided', () => {
        const payload: StoreWorkerPayload = {
          op: 'chaser:start',
          data: {
            symbol: 'BTCUSDT',
            side: OrderSide.Sell,
            size: 2.0,
          },
        };

        handler.handle(payload);

        expect(ChaserOrderInstance).toHaveBeenCalledWith(
          mockExchange,
          expect.objectContaining({
            accountName: 'test-account',
          }),
        );
      });
    });

    describe('chaser:terminate', () => {
      it('should call remove on manager with id', () => {
        const payload: StoreWorkerPayload = {
          op: 'chaser:terminate',
          data: 'chaser-123',
        };

        handler.handle(payload);

        expect(chaserOrderManagerInstance.remove).toHaveBeenCalledWith('chaser-123');
      });
    });

    describe('other ops', () => {
      it('should not call manager methods for unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: {},
        };

        handler.handle(payload);

        expect(chaserOrderManagerInstance.add).not.toHaveBeenCalled();
        expect(chaserOrderManagerInstance.remove).not.toHaveBeenCalled();
      });
    });
  });

  it('refuses a chase visibly when native account state cannot be read', () => {
    const emit = vi.fn();
    (mockExchange as unknown as { emitter: { emit: typeof emit } }).emitter = { emit };
    vi.mocked(ChaserOrderInstance).mockImplementationOnce(function () {
      throw new Error('Native market catalog is not ready');
    });
    expect(() =>
      handler.handle({ op: 'chaser:start', data: { symbol: 'BTCUSDT', side: OrderSide.Buy, size: 1 } } as StoreWorkerPayload),
    ).not.toThrow();
    expect(chaserOrderManagerInstance.add).not.toHaveBeenCalled();
    expect(emit).toHaveBeenCalledWith('error', 'Chase for BTCUSDT was not started: Native market catalog is not ready');
  });
});
