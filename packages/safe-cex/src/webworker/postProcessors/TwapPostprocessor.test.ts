import type { Exchange } from '../../exchanges/base';
import type { ExchangeName, ExchangeOptions, Market, StoreData, Writable } from '../../types';
import type { TwapInstance } from '../twap/TwapInstance';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';
import { twapManager } from '../twap/TwapManager';
import { TwapStatus } from '../twap/TwapStatus';
import { TwapPostprocessor } from './TwapPostprocessor';

// Mock the twapManager
vi.mock('../twap/TwapManager', () => ({
  twapManager: {
    runningTwaps: new Map(),
  },
}));

describe('TwapPostprocessor', () => {
  let processor: TwapPostprocessor;
  let mockExchange: Exchange;
  let mockStore: Writable<StoreData>;

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

  const createMockTwapInstance = (
    id: string,
    side: OrderSide = OrderSide.Buy,
    size: number = 1.0,
    activeMarket: Market | null = mockMarket,
  ): TwapInstance => {
    return {
      id,
      options: {
        accountName: 'test-account',
        activeMarket,
        size,
        duration: 60000,
        ordersCount: 10,
        side,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: true,
        useChaserOrders: false,
      },
      state: {
        accountName: 'test-account',
        activeMarket,
        size,
        ordersSizes: [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1],
        side,
        status: TwapStatus.Running,
        ordersCount: 10,
        ordersExecuted: 0,
        nextOrderAt: Date.now() + 6000,
      },
    } as TwapInstance;
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockExchange = {} as Exchange;
    processor = new TwapPostprocessor('test-account', 'bybit' as ExchangeName, {} as ExchangeOptions, mockExchange);

    mockStore = {
      activeTWAPs: [],
    } as unknown as Writable<StoreData>;

    // Clear the mock Map
    twapManager.runningTwaps.clear();
  });

  describe('process', () => {
    it('should set activeTWAPs to empty array when no running TWAPs', () => {
      processor.process(mockStore);

      expect(mockStore.activeTWAPs).toEqual([]);
    });

    it('should process running TWAPs with valid markets', () => {
      const twap = createMockTwapInstance('twap-1', OrderSide.Buy, 1.0);
      twapManager.runningTwaps.set('twap-1', twap);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs).toHaveLength(1);
      expect(mockStore.activeTWAPs[0].id).toBe('twap-1');
      expect(mockStore.activeTWAPs[0].symbol).toBe('BTCUSDT');
    });

    it('should filter out TWAPs with Completed status', () => {
      const runningTwap = createMockTwapInstance('twap-running', OrderSide.Buy, 1.0);
      const completedTwap = createMockTwapInstance('twap-completed', OrderSide.Buy, 1.0);
      completedTwap.state.status = TwapStatus.Completed;

      twapManager.runningTwaps.set('twap-running', runningTwap);
      twapManager.runningTwaps.set('twap-completed', completedTwap);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs).toHaveLength(1);
      expect(mockStore.activeTWAPs[0].id).toBe('twap-running');
    });

    it('should filter out TWAPs without activeMarket', () => {
      const validTwap = createMockTwapInstance('twap-1', OrderSide.Buy, 1.0, mockMarket);
      const invalidTwap = createMockTwapInstance('twap-2', OrderSide.Buy, 1.0, null);

      twapManager.runningTwaps.set('twap-1', validTwap);
      twapManager.runningTwaps.set('twap-2', invalidTwap);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs).toHaveLength(1);
      expect(mockStore.activeTWAPs[0].id).toBe('twap-1');
    });

    it('should negate size for sell orders', () => {
      const sellTwap = createMockTwapInstance('twap-sell', OrderSide.Sell, 2.5);
      twapManager.runningTwaps.set('twap-sell', sellTwap);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs[0].size).toBe(-2.5);
    });

    it('should keep size positive for buy orders', () => {
      const buyTwap = createMockTwapInstance('twap-buy', OrderSide.Buy, 3.0);
      twapManager.runningTwaps.set('twap-buy', buyTwap);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs[0].size).toBe(3.0);
    });

    it('should handle multiple TWAPs', () => {
      const twap1 = createMockTwapInstance('twap-1', OrderSide.Buy, 1.0);
      const twap2 = createMockTwapInstance('twap-2', OrderSide.Sell, 2.0);
      const twap3 = createMockTwapInstance('twap-3', OrderSide.Buy, 0.5);

      twapManager.runningTwaps.set('twap-1', twap1);
      twapManager.runningTwaps.set('twap-2', twap2);
      twapManager.runningTwaps.set('twap-3', twap3);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs).toHaveLength(3);

      const sizes = mockStore.activeTWAPs.map((t: any) => t.size);
      expect(sizes).toContain(1.0);
      expect(sizes).toContain(-2.0); // Negated sell
      expect(sizes).toContain(0.5);
    });

    it('should include both options and state in output', () => {
      const twap = createMockTwapInstance('twap-1', OrderSide.Buy, 1.0);
      (twap as any).currentOrderIds = ['order-1'];
      twapManager.runningTwaps.set('twap-1', twap);

      processor.process(mockStore);

      const activeTwap = mockStore.activeTWAPs[0];

      // From options
      expect(activeTwap.duration).toBe(60000);
      expect(activeTwap.ordersCount).toBe(10);
      expect(activeTwap.reduceOnly).toBe(false);

      // From state
      expect(activeTwap.status).toBe(TwapStatus.Running);
      expect(activeTwap.ordersExecuted).toBe(0);
      expect((activeTwap as any).currentOrderIds).toEqual(['order-1']);
    });

    it('should use "Unknown" symbol when market has no symbol', () => {
      const marketWithoutSymbol = { ...mockMarket, symbol: undefined } as any;
      const twap = createMockTwapInstance('twap-1', OrderSide.Buy, 1.0, marketWithoutSymbol);
      twapManager.runningTwaps.set('twap-1', twap);

      processor.process(mockStore);

      expect(mockStore.activeTWAPs[0].symbol).toBe('Unknown');
    });
  });
});
