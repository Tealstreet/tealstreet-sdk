import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide, OrderStopTriggerType } from '../../types';
import { TwapInstance } from '../twap/TwapInstance';
import { twapManager } from '../twap/TwapManager';
import { TwapHandler } from './TwapHandler';

// Track mock function calls and instance creation globally
const mockCalls = {
  start: [] as any[],
  resume: [] as any[],
  pause: [] as any[],
  terminate: [] as any[],
  cleanup: [] as any[],
  instances: [] as any[],
};

// Mock TwapInstance as a class
vi.mock('../twap/TwapInstance', () => ({
  TwapInstance: class MockTwapInstance {
    id: string;
    options: any;
    state: any;
    constructorArgs: any[];

    constructor(exchange: any, ctx: any, options: any, onStateUpdate: any, state: any, id?: string) {
      this.id = id ?? 'mock-twap-id';
      this.options = options;
      this.state = state;
      this.constructorArgs = [exchange, ctx, options, onStateUpdate, state, id];
      mockCalls.instances.push(this);
    }

    start() {
      mockCalls.start.push(this);
    }
    resume() {
      mockCalls.resume.push(this);
    }
    pause() {
      mockCalls.pause.push(this);
    }
    terminate() {
      mockCalls.terminate.push(this);
    }
    cleanup() {
      mockCalls.cleanup.push(this);
    }
  },
}));

// Mock TwapManager
vi.mock('../twap/TwapManager', () => ({
  twapManager: {
    add: vi.fn(),
    getById: vi.fn(),
    runningTwaps: new Map(),
    serializeState: vi.fn(() => ({})),
  },
}));

describe('TwapHandler', () => {
  let handler: TwapHandler;
  let mockExchange: Exchange;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;

  beforeEach(() => {
    vi.clearAllMocks();
    (twapManager.runningTwaps as Map<string, any>).clear();

    // Clear mock call tracking
    mockCalls.start.length = 0;
    mockCalls.resume.length = 0;
    mockCalls.pause.length = 0;
    mockCalls.terminate.length = 0;
    mockCalls.cleanup.length = 0;
    mockCalls.instances.length = 0;

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

    handler = new TwapHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('twap:start', () => {
      it('should create TwapInstance with options and add to manager', () => {
        const options = {
          accountName: 'test-account',
          activeMarket: { symbol: 'BTCUSDT' } as any,
          size: 10,
          duration: 1000,
          ordersCount: 5,
          side: OrderSide.Buy,
          chaserDelay: 1000,
          reduceOnly: false,
          randomnessPercentage: 10,
          useLimitOrders: true,
          useChaserOrders: false,
          origin: 'web' as const,
        };

        const payload: StoreWorkerPayload = {
          op: 'twap:start',
          data: options,
        };

        handler.handle(payload);

        // Check TwapInstance was created with correct args
        expect(mockCalls.instances.length).toBe(1);
        const instance = mockCalls.instances[0];
        expect(instance.constructorArgs[0]).toBe(mockExchange);
        expect(instance.constructorArgs[1]).toBe(mockCtx);
        expect(instance.constructorArgs[2]).toBe(options);
        expect(typeof instance.constructorArgs[3]).toBe('function');
        expect(instance.constructorArgs[4]).toBeUndefined();

        expect(vi.mocked(twapManager).add).toHaveBeenCalled();
        expect(mockCalls.start.length).toBeGreaterThan(0);
      });
    });

    describe('twap:resume', () => {
      it('should call resume on twap instance by id', () => {
        const mockInstance = {
          resume: () => {
            mockCalls.resume.push(mockInstance);
          },
        };
        vi.mocked(twapManager).getById.mockReturnValue(mockInstance as any);

        const payload: StoreWorkerPayload = {
          op: 'twap:resume',
          data: 'twap-123',
        };

        handler.handle(payload);

        expect(vi.mocked(twapManager).getById).toHaveBeenCalledWith('twap-123');
        expect(mockCalls.resume.length).toBeGreaterThan(0);
      });

      it('should not throw when twap not found', () => {
        vi.mocked(twapManager).getById.mockReturnValue(undefined);

        const payload: StoreWorkerPayload = {
          op: 'twap:resume',
          data: 'non-existent',
        };

        expect(() => handler.handle(payload)).not.toThrow();
      });
    });

    describe('twap:cleanup', () => {
      it('should call cleanup on twap instance by id', () => {
        const mockInstance = {
          cleanup: () => {
            mockCalls.cleanup.push(mockInstance);
          },
        };
        vi.mocked(twapManager).getById.mockReturnValue(mockInstance as any);

        const payload: StoreWorkerPayload = {
          op: 'twap:cleanup',
          data: 'twap-456',
        };

        handler.handle(payload);

        expect(vi.mocked(twapManager).getById).toHaveBeenCalledWith('twap-456');
        expect(mockCalls.cleanup.length).toBeGreaterThan(0);
      });
    });

    describe('twap:pause', () => {
      it('should call pause on twap instance by id', () => {
        const mockInstance = {
          pause: () => {
            mockCalls.pause.push(mockInstance);
          },
        };
        vi.mocked(twapManager).getById.mockReturnValue(mockInstance as any);

        const payload: StoreWorkerPayload = {
          op: 'twap:pause',
          data: 'twap-789',
        };

        handler.handle(payload);

        expect(vi.mocked(twapManager).getById).toHaveBeenCalledWith('twap-789');
        expect(mockCalls.pause.length).toBeGreaterThan(0);
      });
    });

    describe('twap:stop', () => {
      it('should call terminate on twap instance by id', () => {
        const mockInstance = {
          terminate: () => {
            mockCalls.terminate.push(mockInstance);
          },
        };
        vi.mocked(twapManager).getById.mockReturnValue(mockInstance as any);

        const payload: StoreWorkerPayload = {
          op: 'twap:stop',
          data: 'twap-abc',
        };

        handler.handle(payload);

        expect(vi.mocked(twapManager).getById).toHaveBeenCalledWith('twap-abc');
        expect(mockCalls.terminate.length).toBeGreaterThan(0);
      });
    });

    describe('twap:state', () => {
      it('should restore persisted twap states', () => {
        const persistedStates = [
          {
            id: 'twap-restored-1',
            options: {
              symbol: 'BTCUSDT',
              side: OrderSide.Buy,
              totalSize: 5,
            },
            state: {
              status: 'paused',
              filledSize: 2,
            },
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'twap:state',
          data: persistedStates,
        };

        handler.handle(payload);

        // Check TwapInstance was created with correct args
        expect(mockCalls.instances.length).toBe(1);
        const instance = mockCalls.instances[0];
        expect(instance.constructorArgs[0]).toBe(mockExchange);
        expect(instance.constructorArgs[1]).toBe(mockCtx);
        expect(instance.constructorArgs[2]).toBe(persistedStates[0].options);
        expect(typeof instance.constructorArgs[3]).toBe('function');
        expect(instance.constructorArgs[4]).toBe(persistedStates[0].state);
        expect(instance.constructorArgs[5]).toBe('twap-restored-1');

        expect(vi.mocked(twapManager).add).toHaveBeenCalled();
      });

      it('should resume running twaps after restore', () => {
        const persistedStates = [
          {
            id: 'twap-running',
            options: { symbol: 'ETHUSDT' },
            state: { status: 'running' },
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'twap:state',
          data: persistedStates,
        };

        handler.handle(payload);

        // Should pause then resume running twaps
        expect(mockCalls.pause.length).toBeGreaterThan(0);
        expect(mockCalls.resume.length).toBeGreaterThan(0);
      });

      it('should not restore already existing twaps', () => {
        // Setup existing twap in manager
        const existingTwap = { id: 'existing-twap' };
        (twapManager.runningTwaps as Map<string, any>).set('existing-twap', existingTwap);

        const persistedStates = [
          {
            id: 'existing-twap',
            options: { symbol: 'BTCUSDT' },
            state: { status: 'paused' },
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'twap:state',
          data: persistedStates,
        };

        handler.handle(payload);

        // TwapInstance should not be called because twap already exists
        expect(mockCalls.instances.length).toBe(0);
      });

      it('should continue restoring non-existing twaps after existing ids', () => {
        const existingTwap = { id: 'existing-twap' };
        (twapManager.runningTwaps as Map<string, any>).set('existing-twap', existingTwap);

        const persistedStates = [
          {
            id: 'existing-twap',
            options: { symbol: 'BTCUSDT' },
            state: { status: 'paused' },
          },
          {
            id: 'new-twap',
            options: { symbol: 'ETHUSDT' },
            state: { status: 'paused' },
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'twap:state',
          data: persistedStates,
        };

        handler.handle(payload);

        expect(mockCalls.instances.length).toBe(1);
        expect(mockCalls.instances[0].constructorArgs[5]).toBe('new-twap');
      });

      it('should handle errors gracefully', () => {
        const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

        const payload: StoreWorkerPayload = {
          op: 'twap:state',
          data: null, // Invalid data
        };

        // Should not throw
        expect(() => handler.handle(payload)).not.toThrow();

        consoleSpy.mockRestore();
      });
    });

    describe('other ops', () => {
      it('should not call manager methods for unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: {},
        };

        handler.handle(payload);

        expect(vi.mocked(twapManager).add).not.toHaveBeenCalled();
        expect(vi.mocked(twapManager).getById).not.toHaveBeenCalled();
      });
    });
  });
});
