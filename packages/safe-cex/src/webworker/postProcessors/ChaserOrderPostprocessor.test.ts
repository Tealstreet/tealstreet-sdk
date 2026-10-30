import type { Exchange } from '../../exchanges/base';
import type { ActiveChaser, ExchangeName, ExchangeOptions, StoreData, Writable } from '../../types';
import type { ChaserOrderInstance } from '../chaserOrder/ChaserOrderInstance';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';
import { chaserOrderManagerInstance } from '../chaserOrder/ChaserOrderManager';
import { ChaserOrderPostprocessor } from './ChaserOrderPostprocessor';

// Mock the chaserOrderManagerInstance
vi.mock('../chaserOrder/ChaserOrderManager', () => ({
  chaserOrderManagerInstance: {
    instances: [],
  },
}));

describe('ChaserOrderPostprocessor', () => {
  let processor: ChaserOrderPostprocessor;
  let mockExchange: Exchange;
  let mockStore: Writable<StoreData>;

  const createMockChaserInstance = (
    id: string,
    exchange: Exchange,
    symbol: string = 'BTCUSDT',
    side: OrderSide = OrderSide.Buy,
    size: number = 1.0,
  ): ChaserOrderInstance => {
    return {
      id,
      exchange,
      options: {
        symbol,
        side,
        size,
        maxAttempts: 10,
        reduceOnly: false,
        origin: 'web',
      },
      state: {
        attempts: 3,
        isStopped: false,
        stopReason: '',
      },
      currentOrderIds: ['order-1', 'order-2'],
    } as unknown as ChaserOrderInstance;
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockExchange = { name: 'bybit' } as Exchange;
    processor = new ChaserOrderPostprocessor(
      'test-account',
      'bybit' as ExchangeName,
      {} as ExchangeOptions,
      mockExchange,
    );

    mockStore = {
      activeChasers: [],
    } as unknown as Writable<StoreData>;

    // Clear the instances array (access private property)
    (chaserOrderManagerInstance as any)._chaserOrderInstances = [];
  });

  describe('process', () => {
    it('should set activeChasers to empty array when no instances', () => {
      processor.process(mockStore);

      expect(mockStore.activeChasers).toEqual([]);
    });

    it('should only include chasers for the current exchange', () => {
      const otherExchange = { name: 'binance' } as Exchange;

      const chaserForThisExchange = createMockChaserInstance('chaser-1', mockExchange);
      const chaserForOtherExchange = createMockChaserInstance('chaser-2', otherExchange);

      (chaserOrderManagerInstance as any).instances = [chaserForThisExchange, chaserForOtherExchange];

      processor.process(mockStore);

      expect(mockStore.activeChasers).toHaveLength(1);
      expect(mockStore.activeChasers[0].id).toBe('chaser-1');
    });

    it('should map chaser instance to ActiveChaser format', () => {
      const chaser = createMockChaserInstance('chaser-1', mockExchange, 'ETHUSDT', OrderSide.Sell, 2.5);
      chaser.options.maxAttempts = 15;
      chaser.options.reduceOnly = true;
      chaser.state.attempts = 5;
      chaser.state.isStopped = false;
      chaser.state.stopReason = '';
      chaser.currentOrderIds = ['order-x', 'order-y', 'order-z'];
      chaser.options.origin = 'webhook';

      (chaserOrderManagerInstance as any).instances = [chaser];

      processor.process(mockStore);

      expect(mockStore.activeChasers).toHaveLength(1);

      const activeChaser = mockStore.activeChasers[0];
      expect(activeChaser.id).toBe('chaser-1');
      expect(activeChaser.symbol).toBe('ETHUSDT');
      expect(activeChaser.size).toBe(2.5);
      expect(activeChaser.side).toBe(OrderSide.Sell);
      expect(activeChaser.maxAttempts).toBe(15);
      expect(activeChaser.reduceOnly).toBe(true);
      expect(activeChaser.attempts).toBe(5);
      expect(activeChaser.isStopped).toBe(false);
      expect(activeChaser.stopReason).toBe('');
      expect(activeChaser.currentOrderIds).toEqual(['order-x', 'order-y', 'order-z']);
      expect(activeChaser.origin).toBe('webhook');
    });

    it('excludes stopped instances while asynchronous cleanup finishes', () => {
      const chaser = createMockChaserInstance('chaser-1', mockExchange);
      chaser.state.isStopped = true;
      (chaserOrderManagerInstance as any).instances = [chaser];

      processor.process(mockStore);

      expect(mockStore.activeChasers).toEqual([]);
    });

    it('should handle multiple chasers for the same exchange', () => {
      const chaser1 = createMockChaserInstance('chaser-1', mockExchange, 'BTCUSDT');
      const chaser2 = createMockChaserInstance('chaser-2', mockExchange, 'ETHUSDT');
      const chaser3 = createMockChaserInstance('chaser-3', mockExchange, 'SOLUSDT');

      (chaserOrderManagerInstance as any).instances = [chaser1, chaser2, chaser3];

      processor.process(mockStore);

      expect(mockStore.activeChasers).toHaveLength(3);

      const symbols = mockStore.activeChasers.map((c: ActiveChaser) => c.symbol);
      expect(symbols).toContain('BTCUSDT');
      expect(symbols).toContain('ETHUSDT');
      expect(symbols).toContain('SOLUSDT');
    });

    it('should handle empty currentOrderIds', () => {
      const chaser = createMockChaserInstance('chaser-1', mockExchange);
      chaser.currentOrderIds = [];

      (chaserOrderManagerInstance as any).instances = [chaser];

      processor.process(mockStore);

      expect(mockStore.activeChasers[0].currentOrderIds).toEqual([]);
    });

    it('should handle buy side', () => {
      const chaser = createMockChaserInstance('chaser-buy', mockExchange, 'BTCUSDT', OrderSide.Buy);

      (chaserOrderManagerInstance as any).instances = [chaser];

      processor.process(mockStore);

      expect(mockStore.activeChasers[0].side).toBe(OrderSide.Buy);
    });

    it('should handle sell side', () => {
      const chaser = createMockChaserInstance('chaser-sell', mockExchange, 'BTCUSDT', OrderSide.Sell);

      (chaserOrderManagerInstance as any).instances = [chaser];

      processor.process(mockStore);

      expect(mockStore.activeChasers[0].side).toBe(OrderSide.Sell);
    });

    it('should preserve reduceOnly flag', () => {
      const chaserReduceOnly = createMockChaserInstance('chaser-1', mockExchange);
      chaserReduceOnly.options.reduceOnly = true;

      const chaserNotReduceOnly = createMockChaserInstance('chaser-2', mockExchange);
      chaserNotReduceOnly.options.reduceOnly = false;

      (chaserOrderManagerInstance as any).instances = [chaserReduceOnly, chaserNotReduceOnly];

      processor.process(mockStore);

      const reduceOnlyChaser = mockStore.activeChasers.find((c: ActiveChaser) => c.id === 'chaser-1');
      const normalChaser = mockStore.activeChasers.find((c: ActiveChaser) => c.id === 'chaser-2');

      expect(reduceOnlyChaser?.reduceOnly).toBe(true);
      expect(normalChaser?.reduceOnly).toBe(false);
    });
  });
});
