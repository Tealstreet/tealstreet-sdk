import type { TwapOptions, TwapOrigin } from './TwapOptions';
import type { TwapState } from './TwapState';

import { describe, expect, it } from 'vitest';

import { Market, OrderSide, PositionSide } from '../../types';
import { TwapStatus } from './TwapStatus';

describe('Twap Types', () => {
  const mockMarket: Market = {
    symbol: 'BTCUSDT',
    id: 'btcusdt',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 3, price: 2 },
    limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
  };

  describe('TwapOptions', () => {
    it('should allow creating valid options with required fields', () => {
      const options: TwapOptions = {
        accountName: 'main-account',
        activeMarket: mockMarket,
        size: 1.0,
        duration: 60000, // 1 minute
        ordersCount: 10,
        side: OrderSide.Buy,
        chaserDelay: 1000,
        reduceOnly: false,
        randomnessPercentage: 10,
        useLimitOrders: true,
        useChaserOrders: false,
      };

      expect(options.accountName).toBe('main-account');
      expect(options.activeMarket.symbol).toBe('BTCUSDT');
      expect(options.size).toBe(1.0);
      expect(options.duration).toBe(60000);
      expect(options.ordersCount).toBe(10);
      expect(options.side).toBe(OrderSide.Buy);
    });

    it('should allow options with origin field', () => {
      const options: TwapOptions = {
        accountName: 'main-account',
        activeMarket: mockMarket,
        size: 0.5,
        duration: 120000,
        ordersCount: 5,
        side: OrderSide.Sell,
        chaserDelay: 500,
        reduceOnly: true,
        randomnessPercentage: 5,
        useLimitOrders: false,
        useChaserOrders: true,
        origin: 'cli',
      };

      expect(options.origin).toBe('cli');
      expect(options.reduceOnly).toBe(true);
      expect(options.useChaserOrders).toBe(true);
    });
  });

  describe('TwapOrigin', () => {
    it('should accept valid origin types', () => {
      const origins: TwapOrigin[] = ['web', 'cli', 'webhook', 'automation'];

      origins.forEach((origin) => {
        const options: TwapOptions = {
          accountName: 'test',
          activeMarket: mockMarket,
          size: 1,
          duration: 60000,
          ordersCount: 10,
          side: OrderSide.Buy,
          chaserDelay: 1000,
          reduceOnly: false,
          randomnessPercentage: 0,
          useLimitOrders: true,
          useChaserOrders: false,
          origin,
        };
        expect(options.origin).toBe(origin);
      });
    });
  });

  describe('TwapState', () => {
    it('should allow creating an idle state', () => {
      const state: TwapState = {
        accountName: 'main-account',
        activeMarket: mockMarket,
        size: 1.0,
        ordersSizes: [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1],
        side: OrderSide.Buy,
        status: TwapStatus.Idle,
        ordersCount: 10,
        ordersExecuted: 0,
        nextOrderAt: 0,
      };

      expect(state.status).toBe(TwapStatus.Idle);
      expect(state.ordersExecuted).toBe(0);
      expect(state.ordersSizes).toHaveLength(10);
    });

    it('should allow creating a running state', () => {
      const state: TwapState = {
        accountName: 'main-account',
        activeMarket: mockMarket,
        size: 1.0,
        ordersSizes: [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1],
        side: OrderSide.Sell,
        status: TwapStatus.Running,
        ordersCount: 10,
        ordersExecuted: 3,
        nextOrderAt: Date.now() + 6000,
        startedAt: Date.now(),
        createdAt: Date.now() - 1000,
      };

      expect(state.status).toBe(TwapStatus.Running);
      expect(state.ordersExecuted).toBe(3);
      expect(state.startedAt).toBeDefined();
    });

    it('should allow creating a paused state', () => {
      const state: TwapState = {
        accountName: 'main-account',
        activeMarket: mockMarket,
        size: 0.5,
        ordersSizes: [0.1, 0.1, 0.1, 0.1, 0.1],
        side: OrderSide.Buy,
        status: TwapStatus.Paused,
        ordersCount: 5,
        ordersExecuted: 2,
        nextOrderAt: 0,
      };

      expect(state.status).toBe(TwapStatus.Paused);
    });

    it('should allow creating a completed state', () => {
      const state: TwapState = {
        accountName: 'main-account',
        activeMarket: mockMarket,
        size: 1.0,
        ordersSizes: [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1],
        side: OrderSide.Buy,
        status: TwapStatus.Completed,
        ordersCount: 10,
        ordersExecuted: 10,
        nextOrderAt: 0,
        markForCleanup: true,
      };

      expect(state.status).toBe(TwapStatus.Completed);
      expect(state.ordersExecuted).toBe(state.ordersCount);
      expect(state.markForCleanup).toBe(true);
    });

    it('should allow state with origin', () => {
      const state: TwapState = {
        accountName: 'webhook-account',
        activeMarket: mockMarket,
        size: 2.0,
        ordersSizes: [0.5, 0.5, 0.5, 0.5],
        side: OrderSide.Sell,
        status: TwapStatus.Running,
        ordersCount: 4,
        ordersExecuted: 1,
        nextOrderAt: Date.now() + 15000,
        origin: 'webhook',
        createdAt: Date.now() - 5000,
      };

      expect(state.origin).toBe('webhook');
    });
  });
});
