import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ChaserOrderInstance, defaultChaserState } from './ChaserOrderInstance';
import { ChaserOrderOptions, ChaserOrigin } from './ChaserOrderOptions';
import { OrderSide } from '../../types';
import { Exchange } from '../../exchanges/base';

// Mock exchange
const createMockExchange = () =>
  ({
    store: {
      orders: [],
      tickers: [],
    },
    placeOrder: vi.fn().mockResolvedValue({ id: 'order-1' }),
    cancelOrder: vi.fn().mockResolvedValue(undefined),
  }) as unknown as Exchange;

describe('ChaserOrderInstance', () => {
  let mockExchange: Exchange;

  beforeEach(() => {
    mockExchange = createMockExchange();
  });

  it('captures the native position before a pending streaming flush', () => {
    const store = { positions: [] as Array<{ symbol: string; side: string; contracts: number }> };
    const syncAccountState = vi.fn(() => {
      store.positions = [{ symbol: 'BTCUSDT', side: 'long', contracts: 4 }];
    });
    const exchange = { store, syncAccountState } as unknown as Exchange;
    const instance = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT', size: 2, side: OrderSide.Buy, maxAttempts: 10, delay: 100,
    });
    expect(syncAccountState).toHaveBeenCalledOnce();
    expect(instance.targetPositionContracts).toBe(6);
  });

  describe('Backwards Compatibility - Origin Field', () => {
    it('should create instance without origin (backwards compatibility)', () => {
      // This simulates old code that doesn't pass origin
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
        // origin NOT provided - simulating old code
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.options.origin).toBeUndefined();
      expect(instance.id).toBeDefined();
      expect(instance.state).toEqual(defaultChaserState);
    });

    it('should accept and preserve origin when provided', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
        origin: 'cli',
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.options.origin).toBe('cli');
    });

    it('should support all origin types', () => {
      const origins: ChaserOrigin[] = ['web', 'cli', 'webhook', 'automation'];

      origins.forEach((origin) => {
        const options: ChaserOrderOptions = {
          symbol: 'BTCUSDT',
          size: 0.1,
          side: OrderSide.Buy,
          maxAttempts: 100,
          reduceOnly: false,
          delay: 1000,
          origin,
        };

        const instance = new ChaserOrderInstance(mockExchange, options);

        expect(instance.options.origin).toBe(origin);
      });
    });
  });

  describe('Instance Creation', () => {
    it('should generate unique ID', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance1 = new ChaserOrderInstance(mockExchange, options);
      const instance2 = new ChaserOrderInstance(mockExchange, options);

      expect(instance1.id).not.toBe(instance2.id);
    });

    it('should initialize with default state', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.state.attempts).toBe(0);
      expect(instance.state.isStopped).toBe(false);
      expect(instance.state.stopReason).toBe('');
      expect(instance.state.currentOrdersIds).toEqual([]);
    });

    it('should set initial remaining size to options size', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.5,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.remainingSize).toBe(0.5);
    });
  });

  describe('State Management', () => {
    it('should increment attempts', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.state.attempts).toBe(0);
      instance.incrementAttempts();
      expect(instance.state.attempts).toBe(1);
      instance.incrementAttempts();
      expect(instance.state.attempts).toBe(2);
    });

    it('should stop when max attempts reached', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 2,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      // The code checks attempts > maxAttempts BEFORE incrementing
      // So with maxAttempts=2:
      // - incrementAttempts(): 0 not > 2, increment to 1
      // - incrementAttempts(): 1 not > 2, increment to 2
      // - incrementAttempts(): 2 not > 2, increment to 3
      // - incrementAttempts(): 3 > 2, stop (don't increment)
      instance.incrementAttempts(); // attempts = 1
      instance.incrementAttempts(); // attempts = 2
      instance.incrementAttempts(); // attempts = 3
      expect(instance.state.isStopped).toBe(false);

      instance.incrementAttempts(); // 3 > 2, stops

      expect(instance.state.isStopped).toBe(true);
      expect(instance.state.stopReason).toBe('Reached maximum number of attempts');
    });

    it('should mark complete with custom reason', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      instance.markComplete('Order filled');

      expect(instance.state.isStopped).toBe(true);
      expect(instance.state.stopReason).toBe('Order filled');
    });

    it('should track current order IDs', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      instance.currentOrderIds = ['order-1', 'order-2'];

      expect(instance.currentOrderIds).toEqual(['order-1', 'order-2']);
    });
  });

  describe('Fill Tracking', () => {
    it('should subtract filled size from remaining', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 1.0,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      // Simulate partial fill
      instance.subtractFilledSize({ id: 'order-1', filled: 0.3 } as any);

      expect(instance.remainingSize).toBeCloseTo(0.7, 10);
    });

    it('should stop when fully filled', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 1.0,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      instance.subtractFilledSize({ id: 'order-1', filled: 1.0 } as any);

      expect(instance.isStopped).toBe(true);
      expect(instance.state.stopReason).toBe('Fully filled already');
    });

    it('should add fill amounts incrementally', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 1.0,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      instance.addFillAmount('order-1', 0.2);
      expect(instance.remainingSize).toBeCloseTo(0.8, 10);

      instance.addFillAmount('order-1', 0.3);
      expect(instance.remainingSize).toBeCloseTo(0.5, 10);
    });

    it('should set filled amount from REST API', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 1.0,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      // First set via addFillAmount
      instance.addFillAmount('order-1', 0.2);

      // Then REST shows more
      instance.setFilledAmount('order-1', 0.5);

      expect(instance.remainingSize).toBeCloseTo(0.5, 10);
    });

    it('should not decrease filled amount from REST API', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 1.0,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      // Set via REST
      instance.setFilledAmount('order-1', 0.5);
      expect(instance.remainingSize).toBeCloseTo(0.5, 10);

      // Try to set lower amount (should be ignored)
      instance.setFilledAmount('order-1', 0.3);
      expect(instance.remainingSize).toBeCloseTo(0.5, 10);
    });
  });

  describe('Getters', () => {
    it('should return isStopped status', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: false,
        delay: 1000,
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.isStopped).toBe(false);

      instance.markComplete('Done');

      expect(instance.isStopped).toBe(true);
    });

    it('should return options', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 100,
        reduceOnly: true,
        delay: 1000,
        origin: 'webhook',
      };

      const instance = new ChaserOrderInstance(mockExchange, options);

      expect(instance.options).toEqual(options);
    });
  });
});
