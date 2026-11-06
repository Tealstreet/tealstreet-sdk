import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TwapInstance } from './TwapInstance';
import { TwapOptions } from './TwapOptions';
import { TwapState } from './TwapState';
import { TwapStatus } from './TwapStatus';
import { OrderSide, Market } from '../../types';
import { Exchange } from '../../exchanges/base';

// Mock exchange
const createMockExchange = () =>
  ({
    store: {
      tickers: [],
    },
    placeOrder: vi.fn().mockResolvedValue({ id: 'order-1' }),
  }) as unknown as Exchange;

// Mock worker context
const createMockWorkerContext = () =>
  ({
    postMessage: vi.fn(),
  }) as unknown as Worker;

// Mock market
const createMockMarket = (): Market => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: {
    amount: { min: 0.001, max: 1000 },
    leverage: { min: 1, max: 100 },
  },
});

describe('TwapInstance', () => {
  let mockExchange: Exchange;
  let mockWorkerContext: Worker;
  let mockMarket: Market;

  beforeEach(() => {
    mockExchange = createMockExchange();
    mockWorkerContext = createMockWorkerContext();
    mockMarket = createMockMarket();
  });

  describe('Backwards Compatibility - Origin Field', () => {
    it('should create state with default origin "web" when origin not provided', () => {
      // This simulates old code that doesn't pass origin
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
        // origin NOT provided - simulating old code
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      expect(instance.state.origin).toBe('web');
    });

    it('should use provided origin when specified', () => {
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
        origin: 'cli',
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      expect(instance.state.origin).toBe('cli');
    });

    it('should support all origin types', () => {
      const origins = ['web', 'cli', 'webhook', 'automation'] as const;

      origins.forEach((origin) => {
        const options: TwapOptions = {
          accountName: 'test-account',
          activeMarket: mockMarket,
          size: 1.0,
          duration: 10,
          ordersCount: 5,
          side: OrderSide.Buy,
          chaserDelay: 1000,
          reduceOnly: false,
          randomnessPercentage: 10,
          useLimitOrders: false,
          useChaserOrders: false,
          origin,
        };

        const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

        expect(instance.state.origin).toBe(origin);
      });
    });
  });

  describe('State Creation', () => {
    it('should set createdAt timestamp on creation', () => {
      const beforeCreate = Date.now();

      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      const afterCreate = Date.now();

      expect(instance.state.createdAt).toBeDefined();
      expect(instance.state.createdAt).toBeGreaterThanOrEqual(beforeCreate);
      expect(instance.state.createdAt).toBeLessThanOrEqual(afterCreate);
    });

    it('should not set startedAt until start() is called', () => {
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      expect(instance.state.startedAt).toBeUndefined();
    });

    it('should initialize with Idle status', () => {
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      expect(instance.state.status).toBe(TwapStatus.Idle);
    });

    it('should generate correct number of order sizes', () => {
      const ordersCount = 5;
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 0, // No randomness for predictable test
        useLimitOrders: false,
        useChaserOrders: false,
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      expect(instance.state.ordersSizes.length).toBe(ordersCount);
    });

    it('should sum order sizes to total size', () => {
      const totalSize = 1.0;
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: totalSize,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
      };

      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      const sum = instance.state.ordersSizes.reduce((a, b) => a + b, 0);
      expect(sum).toBeCloseTo(totalSize, 10);
    });
  });

  describe('State Restoration', () => {
    it('should restore state from existing state object', () => {
      const existingState: TwapState = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        ordersSizes: [0.2, 0.2, 0.2, 0.2, 0.2],
        side: OrderSide.Buy,
        status: TwapStatus.Paused,
        ordersCount: 5,
        ordersExecuted: 2,
        nextOrderAt: Date.now() + 10000,
        origin: 'cli',
        createdAt: Date.now() - 60000,
        startedAt: Date.now() - 50000,
      };

      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
        origin: 'cli',
      };

      const instance = new TwapInstance(
        mockExchange,
        mockWorkerContext,
        options,
        vi.fn(),
        existingState, // Pass existing state
        'existing-id',
      );

      expect(instance.state.status).toBe(TwapStatus.Paused);
      expect(instance.state.ordersExecuted).toBe(2);
      expect(instance.state.origin).toBe('cli');
      expect(instance.id).toBe('existing-id');
    });

    it('should handle restored state without origin field (old data)', () => {
      // Simulates restoring old state that doesn't have origin field
      const existingState: Partial<TwapState> = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        ordersSizes: [0.2, 0.2, 0.2, 0.2, 0.2],
        side: OrderSide.Buy,
        status: TwapStatus.Running,
        ordersCount: 5,
        ordersExecuted: 2,
        nextOrderAt: Date.now() + 10000,
        // origin NOT present - simulating old serialized state
      };

      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 10,
        ordersCount: 5,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: false,
        useChaserOrders: false,
        // origin NOT provided
      };

      const instance = new TwapInstance(
        mockExchange,
        mockWorkerContext,
        options,
        vi.fn(),
        existingState as TwapState,
        'existing-id',
      );

      // Should not crash and origin should be undefined (will be handled by UI fallback)
      expect(instance.state.status).toBe(TwapStatus.Running);
      expect(instance.state.origin).toBeUndefined();
    });
  });

  describe('Start Behavior', () => {
    it('should set startedAt when start() is called from Idle', async () => {
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 0.001, // Small size for test
        duration: 10,
        ordersCount: 2,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 0,
        useLimitOrders: false,
        useChaserOrders: false,
      };

      const updateCallback = vi.fn();
      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, updateCallback);

      const beforeStart = Date.now();
      await instance.start();
      const afterStart = Date.now();

      expect(instance.state.startedAt).toBeDefined();
      expect(instance.state.startedAt).toBeGreaterThanOrEqual(beforeStart);
      expect(instance.state.startedAt).toBeLessThanOrEqual(afterStart);
      expect(instance.state.status).toBe(TwapStatus.Running);
    });

    it('tracks returned order IDs for direct limit slices', async () => {
      (mockExchange.store as any).tickers = [{ symbol: 'BTCUSDT', bid: 49_900, ask: 50_100, last: 50_000 }];
      (mockExchange.placeOrder as any).mockResolvedValueOnce(['twap-order-1']);
      const options: TwapOptions = {
        accountName: 'test-account',
        activeMarket: mockMarket,
        size: 0.001,
        duration: 10,
        ordersCount: 2,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 0,
        useLimitOrders: true,
        useChaserOrders: false,
      };
      const instance = new TwapInstance(mockExchange, mockWorkerContext, options, vi.fn());

      await instance.placeOrder(0.001);

      expect(instance.currentOrderIds).toEqual(['twap-order-1']);
    });
  });
});
