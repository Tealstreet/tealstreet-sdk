import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';
import type { TwapLockManager } from '../twap/twapLock';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide, OrderStopTriggerType } from '../../types';
import { TwapInstance } from '../twap/TwapInstance';
import { setTwapLockManager, TWAP_RESTORE_LOCK_WAIT_MS } from '../twap/twapLock';
import { twapManager } from '../twap/TwapManager';
import { TwapHandler } from './TwapHandler';

/** Web Locks shared by every worker of one origin; a dead worker's locks are released. */
const locks = (() => {
  const held = new Map<string, string>();
  return {
    reset: () => held.clear(),
    crash: (worker: string) => {
      for (const [name, owner] of held) if (owner === worker) held.delete(name);
    },
    isHeld: (name: string) => held.has(name),
    forWorker: (worker: string): TwapLockManager => ({
      async request(name, _options, callback) {
        if (held.has(name)) return callback(null);
        held.set(name, worker);
        try {
          await callback({ name });
        } finally {
          if (held.get(name) === worker) held.delete(name);
        }
      },
    }),
  };
})();

const settle = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const row = (id: string, status: string, nextOrderAt = Date.now() + 5_000) => ({
  id,
  options: {
    accountName: 'test-account',
    activeMarket: { symbol: 'BTCUSDT' },
    size: 1,
    duration: 10,
    ordersCount: 5,
    side: OrderSide.Buy,
    randomnessPercentage: 0,
  },
  state: {
    accountName: 'test-account',
    activeMarket: { symbol: 'BTCUSDT' },
    size: 1,
    ordersSizes: [0.5, 0.5],
    side: OrderSide.Buy,
    status,
    ordersCount: 5,
    ordersExecuted: 3,
    nextOrderAt,
  },
});

// Track mock function calls and instance creation globally
const mockCalls = {
  start: [] as any[],
  resume: [] as any[],
  pause: [] as any[],
  terminate: [] as any[],
  cleanup: [] as any[],
  instances: [] as any[],
  locks: [] as any[],
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
    async reconcileInFlight() {}
    awaitOwnerLock() {}
    holdOwnerLock(release: () => void) {
      mockCalls.locks.push({ instance: this, release });
    }
  },
}));

// Mock TwapManager
vi.mock('../twap/TwapManager', () => ({
  twapManager: {
    add: vi.fn(function (this: { runningTwaps: Map<string, unknown> }, twap: { id: string }) {
      this.runningTwaps.set(twap.id, twap);
    }),
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
    mockCalls.locks.length = 0;
    locks.reset();
    setTwapLockManager(locks.forWorker('worker-a'));

    mockExchange = { name: 'bybit', emitter: { emit: vi.fn() } } as unknown as Exchange;
    mockCtx = { postMessage: vi.fn() } as unknown as Worker;
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
      it('restores a valid row, holding its cross-tab lock', async () => {
        const persisted = [row('twap-restored-1', 'paused')];
        handler.handle({ op: 'twap:state', data: persisted });
        await settle();
        expect(mockCalls.instances.length).toBe(1);
        const instance = mockCalls.instances[0];
        expect(instance.constructorArgs[0]).toBe(mockExchange);
        expect(instance.constructorArgs[1]).toBe(mockCtx);
        expect(instance.constructorArgs[2]).toBe(persisted[0].options);
        expect(instance.constructorArgs[4]).toBe(persisted[0].state);
        expect(instance.constructorArgs[5]).toBe('twap-restored-1');
        expect(vi.mocked(twapManager).add).toHaveBeenCalled();
        expect(mockCalls.locks.map((lock) => lock.instance)).toEqual([instance]);
        expect(locks.isHeld('tealstreet-twap:test-account:twap-restored-1')).toBe(true);
      });

      it('resumes running twaps after restore', async () => {
        handler.handle({ op: 'twap:state', data: [row('twap-running', 'running')] });
        await settle();
        expect(mockCalls.pause.length).toBe(1);
        expect(mockCalls.resume.length).toBe(1);
      });

      it('does not restore ids this worker already runs, and keeps going', async () => {
        (twapManager.runningTwaps as Map<string, any>).set('existing-twap', { id: 'existing-twap' });
        handler.handle({ op: 'twap:state', data: [row('existing-twap', 'paused'), row('new-twap', 'paused')] });
        await settle();
        expect(mockCalls.instances.map((instance) => instance.constructorArgs[5])).toEqual(['new-twap']);
      });

      it('a re-sent restore in the same tab is not reported as another tab', async () => {
        const data = [row('twap-once', 'running')];
        handler.handle({ op: 'twap:state', data });
        handler.handle({ op: 'twap:state', data });
        await settle();
        expect(mockCalls.instances.length).toBe(1);
        expect(vi.mocked(mockExchange.emitter.emit)).not.toHaveBeenCalled();
      });

      it('a duplicated tab skips a TWAP another tab runs, with a toast', async () => {
        handler.handle({ op: 'twap:state', data: [row('twap-dup', 'running')] });
        await settle();
        expect(mockCalls.instances.length).toBe(1);

        // The duplicate tab: its own worker and manager, the copied sessionStorage row.
        (twapManager.runningTwaps as Map<string, any>).clear();
        setTwapLockManager(locks.forWorker('worker-b'));
        const exchangeB = { name: 'bybit', emitter: { emit: vi.fn() } } as unknown as Exchange;
        const ctxB = { postMessage: vi.fn() } as unknown as Worker;
        const handlerB = new TwapHandler(
          'test-account',
          'bybit' as ExchangeName,
          exchangeB,
          ctxB,
          mockSettings,
          mockMetadata,
        );
        vi.useFakeTimers();
        try {
          handlerB.handle({ op: 'twap:state', data: [row('twap-dup', 'running')] });
          await settle();
          // Still asking: a reloading tab's old worker may not have let go yet.
          expect(mockCalls.terminate).toEqual([]);
          await vi.advanceTimersByTimeAsync(TWAP_RESTORE_LOCK_WAIT_MS + 500);
        } finally {
          vi.useRealTimers();
        }
        // Built synchronously for the worker's ownership tracking, stopped before it ran.
        expect(mockCalls.instances.length).toBe(2);
        expect(mockCalls.terminate).toEqual([mockCalls.instances[1]]);
        expect(mockCalls.resume.length).toBe(1);
        expect(vi.mocked(exchangeB.emitter.emit)).toHaveBeenCalledWith(
          'error',
          'TWAP BTCUSDT not restored: it is running in another tab',
        );
      });

      it('a reloaded tab takes over once its old worker lets go of the lock', async () => {
        handler.handle({ op: 'twap:state', data: [row('twap-reload', 'running')] });
        await settle();
        (twapManager.runningTwaps as Map<string, any>).clear();
        setTwapLockManager(locks.forWorker('worker-b'));
        const exchangeB = { name: 'bybit', emitter: { emit: vi.fn() } } as unknown as Exchange;
        const ctxB = { postMessage: vi.fn() } as unknown as Worker;
        const handlerB = new TwapHandler(
          'test-account',
          'bybit' as ExchangeName,
          exchangeB,
          ctxB,
          mockSettings,
          mockMetadata,
        );
        vi.useFakeTimers();
        try {
          handlerB.handle({ op: 'twap:state', data: [row('twap-reload', 'running')] });
          await settle();
          // The old page's worker is torn down a second into the new one's restore.
          await vi.advanceTimersByTimeAsync(1_000);
          locks.crash('worker-a');
          await vi.advanceTimersByTimeAsync(500);
        } finally {
          vi.useRealTimers();
        }
        expect(mockCalls.terminate).toEqual([]);
        expect(mockCalls.resume.length).toBe(2);
        expect(vi.mocked(exchangeB.emitter.emit)).not.toHaveBeenCalled();
        expect(locks.isHeld('tealstreet-twap:test-account:twap-reload')).toBe(true);
      });

      it('a TWAP whose owner worker died can be restored by another tab', async () => {
        handler.handle({ op: 'twap:state', data: [row('twap-orphan', 'running')] });
        await settle();
        locks.crash('worker-a');
        (twapManager.runningTwaps as Map<string, any>).clear();

        setTwapLockManager(locks.forWorker('worker-b'));
        const exchangeB = { name: 'bybit', emitter: { emit: vi.fn() } } as unknown as Exchange;
        const ctxB = { postMessage: vi.fn() } as unknown as Worker;
        const handlerB = new TwapHandler(
          'test-account',
          'bybit' as ExchangeName,
          exchangeB,
          ctxB,
          mockSettings,
          mockMetadata,
        );
        handlerB.handle({ op: 'twap:state', data: [row('twap-orphan', 'running')] });
        await settle();
        expect(mockCalls.instances.length).toBe(2);
        expect(vi.mocked(exchangeB.emitter.emit)).not.toHaveBeenCalled();
        expect(locks.isHeld('tealstreet-twap:test-account:twap-orphan')).toBe(true);
      });

      it('refuses a running TWAP whose schedule lapsed, with a toast', async () => {
        handler.handle({ op: 'twap:state', data: [row('twap-lapsed', 'running', Date.now() - 2 * 3_600_000)] });
        await settle();
        expect(mockCalls.instances.length).toBe(0);
        expect(vi.mocked(mockExchange.emitter.emit)).toHaveBeenCalledWith(
          'error',
          'TWAP BTCUSDT not restored: its schedule lapsed, so no tab was running it',
        );
        expect(locks.isHeld('tealstreet-twap:test-account:twap-lapsed')).toBe(false);
      });

      it('restores a TWAP whose last slice was in flight, to settle it', async () => {
        const last = row('twap-last', 'running');
        last.state.ordersSizes = [];
        (last.state as Record<string, unknown>).inFlight = { size: 0.5, clientOrderId: 'cid-last' };
        handler.handle({ op: 'twap:state', data: [last] });
        await settle();
        expect(mockCalls.instances.length).toBe(1);
        expect(vi.mocked(mockExchange.emitter.emit)).not.toHaveBeenCalled();
      });

      it('refuses finished, empty and inflated rows', async () => {
        const empty = row('twap-empty', 'paused');
        empty.state.ordersSizes = [];
        const inflated = row('twap-inflated', 'paused');
        inflated.state.ordersSizes = [0.8, 0.8];
        handler.handle({ op: 'twap:state', data: [row('twap-done', 'completed'), empty, inflated] });
        await settle();
        expect(mockCalls.instances.length).toBe(0);
        expect(vi.mocked(mockExchange.emitter.emit).mock.calls.map((call) => call[1])).toEqual([
          'TWAP BTCUSDT not restored: it already completed',
          'TWAP BTCUSDT not restored: no slices remain',
          'TWAP BTCUSDT not restored: its remaining slices exceed the TWAP size',
        ]);
      });

      it('handles malformed data without throwing', async () => {
        const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
        expect(() => handler.handle({ op: 'twap:state', data: null })).not.toThrow();
        await settle();
        expect(mockCalls.instances.length).toBe(0);
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
