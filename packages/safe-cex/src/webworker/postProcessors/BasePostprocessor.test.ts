import type { ExchangeName, ExchangeOptions, StoreData, Writable } from '../../types';

import { describe, expect, it } from 'vitest';

import { BasePostprocessor } from './BasePostprocessor';

// Concrete implementation for testing
class TestPostprocessor extends BasePostprocessor {
  processCallCount = 0;
  lastStore: Writable<StoreData> | null = null;

  process(store: Writable<StoreData>): void {
    this.processCallCount++;
    this.lastStore = store;
  }

  // Expose protected members for testing
  getAccountName(): string {
    return this._accountName;
  }

  getExchangeName(): ExchangeName {
    return this._exchangeName;
  }

  getOptions(): ExchangeOptions {
    return this._options;
  }
}

const mockOptions = {} as ExchangeOptions;

describe('BasePostprocessor', () => {
  describe('constructor', () => {
    it('should store accountName', () => {
      const processor = new TestPostprocessor('my-account', 'bybit' as ExchangeName, mockOptions);

      expect(processor.getAccountName()).toBe('my-account');
    });

    it('should store exchangeName', () => {
      const processor = new TestPostprocessor('my-account', 'binance' as ExchangeName, mockOptions);

      expect(processor.getExchangeName()).toBe('binance');
    });

    it('should store options', () => {
      const opts = { key: 'test' } as unknown as ExchangeOptions;
      const processor = new TestPostprocessor('my-account', 'bybit' as ExchangeName, opts);

      expect(processor.getOptions()).toBe(opts);
    });
  });

  describe('process (abstract method)', () => {
    it('should be callable on concrete implementation', () => {
      const processor = new TestPostprocessor('my-account', 'bybit' as ExchangeName, mockOptions);
      const mockStore = {} as Writable<StoreData>;

      processor.process(mockStore);

      expect(processor.processCallCount).toBe(1);
    });

    it('should receive store parameter', () => {
      const processor = new TestPostprocessor('my-account', 'bybit' as ExchangeName, mockOptions);
      const mockStore = { testData: 'hello' } as unknown as Writable<StoreData>;

      processor.process(mockStore);

      expect(processor.lastStore).toBe(mockStore);
    });

    it('should be callable multiple times', () => {
      const processor = new TestPostprocessor('my-account', 'bybit' as ExchangeName, mockOptions);
      const mockStore = {} as Writable<StoreData>;

      processor.process(mockStore);
      processor.process(mockStore);
      processor.process(mockStore);

      expect(processor.processCallCount).toBe(3);
    });
  });

  describe('different exchange names', () => {
    it('should support bybit', () => {
      const processor = new TestPostprocessor('account', 'bybit' as ExchangeName, mockOptions);
      expect(processor.getExchangeName()).toBe('bybit');
    });

    it('should support binance', () => {
      const processor = new TestPostprocessor('account', 'binance' as ExchangeName, mockOptions);
      expect(processor.getExchangeName()).toBe('binance');
    });

    it('should support okx', () => {
      const processor = new TestPostprocessor('account', 'okx' as ExchangeName, mockOptions);
      expect(processor.getExchangeName()).toBe('okx');
    });

    it('should support hyperliquid', () => {
      const processor = new TestPostprocessor('account', 'hyperliquid' as ExchangeName, mockOptions);
      expect(processor.getExchangeName()).toBe('hyperliquid');
    });
  });
});
