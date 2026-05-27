import { describe, it, expect } from 'vitest';
import { BaseHandler } from './BaseHandler';
import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import { OrderStopTriggerType } from '../../types';

// Concrete implementation for testing
class TestHandler extends BaseHandler {
  handleCallCount = 0;
  lastPayload: StoreWorkerPayload | null = null;

  handle(payload: StoreWorkerPayload): void {
    this.handleCallCount++;
    this.lastPayload = payload;
  }

  dispose(): void {
    // Custom dispose logic if needed
  }

  // Expose protected members for testing
  getAccountName(): string {
    return this._accountName;
  }

  getExchangeName(): ExchangeName {
    return this._exchangeName;
  }

  getExchange(): Exchange {
    return this._exchange;
  }

  getCtx(): Worker {
    return this._ctx;
  }

  getSettings(): AppSettingsWorkerType {
    return this._settings;
  }

  getMetadata(): ExchangeMetadataWorkerType {
    return this._metadata;
  }
}

describe('BaseHandler', () => {
  const mockExchange = { name: 'bybit' } as Exchange;
  const mockCtx = {} as Worker;
  const mockSettings: AppSettingsWorkerType = {
    placeOrder: {
      tpTriggerType: OrderStopTriggerType.MarkPrice,
      slTriggerType: OrderStopTriggerType.LastPrice,
    },
  };
  const mockMetadata: ExchangeMetadataWorkerType = {
    supportedTPSLTriggerTypes: [OrderStopTriggerType.MarkPrice],
    privateProxyIps: ['192.168.1.1'],
    defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
    defaultSLTriggerType: OrderStopTriggerType.LastPrice,
  };

  describe('constructor', () => {
    it('should store accountName', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getAccountName()).toBe('my-account');
    });

    it('should store exchangeName', () => {
      const handler = new TestHandler(
        'my-account',
        'binance' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getExchangeName()).toBe('binance');
    });

    it('should store exchange reference', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getExchange()).toBe(mockExchange);
    });

    it('should store ctx (Worker)', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getCtx()).toBe(mockCtx);
    });

    it('should store settings', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getSettings()).toBe(mockSettings);
    });

    it('should store metadata', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(handler.getMetadata()).toBe(mockMetadata);
    });
  });

  describe('handle (abstract method)', () => {
    it('should be callable on concrete implementation', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      const payload: StoreWorkerPayload = { op: 'test' as any };
      handler.handle(payload);

      expect(handler.handleCallCount).toBe(1);
    });

    it('should receive payload parameter', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      const payload: StoreWorkerPayload = { op: 'test-op' as any, data: { key: 'value' } };
      handler.handle(payload);

      expect(handler.lastPayload).toBe(payload);
    });
  });

  describe('dispose', () => {
    it('should be callable without error', () => {
      const handler = new TestHandler(
        'my-account',
        'bybit' as ExchangeName,
        mockExchange,
        mockCtx,
        mockSettings,
        mockMetadata,
      );

      expect(() => handler.dispose()).not.toThrow();
    });
  });

  describe('different exchange names', () => {
    const exchanges: ExchangeName[] = ['bybit', 'binance', 'okx', 'hyperliquid', 'blofin'] as ExchangeName[];

    exchanges.forEach((exchangeName) => {
      it(`should support ${exchangeName}`, () => {
        const handler = new TestHandler(
          'account',
          exchangeName,
          mockExchange,
          mockCtx,
          mockSettings,
          mockMetadata,
        );
        expect(handler.getExchangeName()).toBe(exchangeName);
      });
    });
  });
});
