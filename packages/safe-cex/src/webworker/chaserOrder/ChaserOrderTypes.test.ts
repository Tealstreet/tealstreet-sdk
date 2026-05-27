import { describe, it, expect } from 'vitest';
import type { ChaserOrderState } from './ChaserOrderState';
import type { ChaserOrderOptions, ChaserOrigin } from './ChaserOrderOptions';
import { OrderSide } from '@tealstreet/safe-cex/src/types';

describe('ChaserOrder Types', () => {
  describe('ChaserOrderState', () => {
    it('should allow creating a valid state object', () => {
      const state: ChaserOrderState = {
        attempts: 0,
        isStopped: false,
        stopReason: '',
        currentOrdersIds: [],
      };

      expect(state.attempts).toBe(0);
      expect(state.isStopped).toBe(false);
      expect(state.stopReason).toBe('');
      expect(state.currentOrdersIds).toEqual([]);
    });

    it('should allow state with active orders', () => {
      const state: ChaserOrderState = {
        attempts: 3,
        isStopped: false,
        stopReason: '',
        currentOrdersIds: ['order-1', 'order-2'],
      };

      expect(state.attempts).toBe(3);
      expect(state.currentOrdersIds).toHaveLength(2);
    });

    it('should allow stopped state with reason', () => {
      const state: ChaserOrderState = {
        attempts: 5,
        isStopped: true,
        stopReason: 'Max attempts reached',
        currentOrdersIds: [],
      };

      expect(state.isStopped).toBe(true);
      expect(state.stopReason).toBe('Max attempts reached');
    });
  });

  describe('ChaserOrderOptions', () => {
    it('should allow creating valid options with required fields', () => {
      const options: ChaserOrderOptions = {
        symbol: 'BTCUSDT',
        size: 0.1,
        side: OrderSide.Buy,
        maxAttempts: 10,
        delay: 1000,
      };

      expect(options.symbol).toBe('BTCUSDT');
      expect(options.size).toBe(0.1);
      expect(options.side).toBe(OrderSide.Buy);
      expect(options.maxAttempts).toBe(10);
      expect(options.delay).toBe(1000);
    });

    it('should allow options with optional fields', () => {
      const options: ChaserOrderOptions = {
        symbol: 'ETHUSDT',
        size: 1.0,
        side: OrderSide.Sell,
        maxAttempts: 5,
        delay: 500,
        reduceOnly: true,
        origin: 'web',
        accountName: 'main-account',
      };

      expect(options.reduceOnly).toBe(true);
      expect(options.origin).toBe('web');
      expect(options.accountName).toBe('main-account');
    });
  });

  describe('ChaserOrigin', () => {
    it('should accept valid origin types', () => {
      const origins: ChaserOrigin[] = ['web', 'cli', 'webhook', 'automation'];

      origins.forEach((origin) => {
        const options: ChaserOrderOptions = {
          symbol: 'BTCUSDT',
          size: 0.1,
          side: OrderSide.Buy,
          maxAttempts: 10,
          delay: 1000,
          origin,
        };
        expect(options.origin).toBe(origin);
      });
    });
  });
});
