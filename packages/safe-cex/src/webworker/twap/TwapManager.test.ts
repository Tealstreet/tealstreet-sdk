import type { TwapInstance } from './TwapInstance';
import type { TwapOptions } from './TwapOptions';
import type { TwapState } from './TwapState';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { Market, OrderSide } from '../../types';
import { TwapStatus } from './TwapStatus';

// We need to test the TwapManager class, but it's exported as a singleton
// So we'll reimport it fresh for each test
describe('TwapManager', () => {
  let twapManager: any;

  const mockMarket: Market = {
    id: 'btcusdt',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 3, price: 2 },
    limits: {
      amount: { min: 0.001, max: 100 },
      leverage: { min: 1, max: 125 },
    },
  };

  const createMockTwapInstance = (id: string, status: TwapStatus = TwapStatus.Running): TwapInstance => {
    const options: TwapOptions = {
      accountName: 'test-account',
      activeMarket: mockMarket,
      size: 1.0,
      duration: 60000,
      ordersCount: 10,
      side: OrderSide.Buy,
      chaserDelay: 1000,
      reduceOnly: false,
      randomnessPercentage: 10,
      useLimitOrders: true,
      useChaserOrders: false,
    };

    const state: TwapState = {
      accountName: 'test-account',
      activeMarket: mockMarket,
      size: 1.0,
      ordersSizes: [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1],
      side: OrderSide.Buy,
      status,
      ordersCount: 10,
      ordersExecuted: 0,
      nextOrderAt: Date.now() + 6000,
    };

    return {
      id,
      options,
      state,
    } as TwapInstance;
  };

  beforeEach(async () => {
    vi.resetModules();
    const module = await import('./TwapManager');
    twapManager = module.twapManager;
    // Clear any existing twaps
    twapManager.runningTwaps.clear();
  });

  describe('hasId', () => {
    it('should return false for non-existent id', () => {
      expect(twapManager.hasId('non-existent')).toBe(false);
    });

    it('should return true for existing id', () => {
      const twap = createMockTwapInstance('twap-1');
      twapManager.add(twap);
      expect(twapManager.hasId('twap-1')).toBe(true);
    });
  });

  describe('add', () => {
    it('should add twap instance and return its id', () => {
      const twap = createMockTwapInstance('twap-1');
      const returnedId = twapManager.add(twap);
      expect(returnedId).toBe('twap-1');
      expect(twapManager.hasId('twap-1')).toBe(true);
    });

    it('should add multiple twap instances', () => {
      const twap1 = createMockTwapInstance('twap-1');
      const twap2 = createMockTwapInstance('twap-2');

      twapManager.add(twap1);
      twapManager.add(twap2);

      expect(twapManager.hasId('twap-1')).toBe(true);
      expect(twapManager.hasId('twap-2')).toBe(true);
    });

    it('should overwrite existing twap with same id', () => {
      const twap1 = createMockTwapInstance('twap-1', TwapStatus.Running);
      const twap2 = createMockTwapInstance('twap-1', TwapStatus.Paused);

      twapManager.add(twap1);
      twapManager.add(twap2);

      const retrieved = twapManager.getById('twap-1');
      expect(retrieved.state.status).toBe(TwapStatus.Paused);
    });
  });

  describe('getById', () => {
    it('should return undefined for non-existent id', () => {
      expect(twapManager.getById('non-existent')).toBeUndefined();
    });

    it('should return the twap instance for existing id', () => {
      const twap = createMockTwapInstance('twap-1');
      twapManager.add(twap);

      const retrieved = twapManager.getById('twap-1');
      expect(retrieved).toBeDefined();
      expect(retrieved.id).toBe('twap-1');
    });
  });

  describe('serializeState', () => {
    it('should return empty object when no twaps exist', () => {
      const result = twapManager.serializeState();
      expect(result).toEqual({});
    });

    it('should serialize running twaps', () => {
      const twap = createMockTwapInstance('twap-1', TwapStatus.Running);
      twapManager.add(twap);

      const result = twapManager.serializeState();

      expect(result['twap-1']).toBeDefined();
      expect(result['twap-1'].options).toEqual(twap.options);
      expect(result['twap-1'].state).toEqual(twap.state);
    });

    it('should serialize paused twaps', () => {
      const twap = createMockTwapInstance('twap-1', TwapStatus.Paused);
      twapManager.add(twap);

      const result = twapManager.serializeState();

      expect(result['twap-1']).toBeDefined();
    });

    it('should serialize idle twaps', () => {
      const twap = createMockTwapInstance('twap-1', TwapStatus.Idle);
      twapManager.add(twap);

      const result = twapManager.serializeState();

      expect(result['twap-1']).toBeDefined();
    });

    it('should NOT serialize completed twaps', () => {
      const twap = createMockTwapInstance('twap-1', TwapStatus.Completed);
      twapManager.add(twap);

      const result = twapManager.serializeState();

      expect(result['twap-1']).toBeUndefined();
      expect(Object.keys(result)).toHaveLength(0);
    });

    it('should only serialize non-completed twaps in mixed state', () => {
      const runningTwap = createMockTwapInstance('twap-running', TwapStatus.Running);
      const completedTwap = createMockTwapInstance('twap-completed', TwapStatus.Completed);
      const pausedTwap = createMockTwapInstance('twap-paused', TwapStatus.Paused);

      twapManager.add(runningTwap);
      twapManager.add(completedTwap);
      twapManager.add(pausedTwap);

      const result = twapManager.serializeState();

      expect(result['twap-running']).toBeDefined();
      expect(result['twap-paused']).toBeDefined();
      expect(result['twap-completed']).toBeUndefined();
      expect(Object.keys(result)).toHaveLength(2);
    });
  });

  describe('runningTwaps Map', () => {
    it('should be a Map instance', () => {
      expect(twapManager.runningTwaps instanceof Map).toBe(true);
    });

    it('should support iteration', () => {
      const twap1 = createMockTwapInstance('twap-1');
      const twap2 = createMockTwapInstance('twap-2');

      twapManager.add(twap1);
      twapManager.add(twap2);

      const ids: string[] = [];
      for (const twap of twapManager.runningTwaps.values()) {
        ids.push(twap.id);
      }

      expect(ids).toContain('twap-1');
      expect(ids).toContain('twap-2');
    });
  });
});
