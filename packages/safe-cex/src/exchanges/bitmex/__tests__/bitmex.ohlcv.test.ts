import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { DefaultStore } from '../../../store/store.base';
import { BitmexExchange } from '../bitmex.exchange';

describe('BitMEX OHLCV', () => {
  let exchange: BitmexExchange;
  let store: DefaultStore;

  beforeEach(() => {
    store = new DefaultStore();

    exchange = new BitmexExchange(
      'bitmex',
      {
        key: 'test-key',
        secret: 'test-secret',
        testnet: false,
        publicDataOnly: true,
      },
      store,
    );
  });

  afterEach(() => {
    if (exchange) {
      exchange.dispose();
    }
  });

  describe('fetchOHLCV', () => {
    it('should fetch OHLCV data using proxy endpoint', async () => {
      const candles = await exchange.fetchOHLCV({
        symbol: 'XBTUSD',
        interval: '1m',
        limit: 5,
      });

      expect(candles).toBeDefined();
      expect(Array.isArray(candles)).toBe(true);

      if (candles.length > 0) {
        const candle = candles[0];
        expect(candle).toHaveProperty('timestamp');
        expect(candle).toHaveProperty('open');
        expect(candle).toHaveProperty('high');
        expect(candle).toHaveProperty('low');
        expect(candle).toHaveProperty('close');
        expect(candle).toHaveProperty('volume');

        // Validate candle structure
        expect(typeof candle.timestamp).toBe('number');
        expect(typeof candle.open).toBe('number');
        expect(typeof candle.high).toBe('number');
        expect(typeof candle.low).toBe('number');
        expect(typeof candle.close).toBe('number');
        expect(typeof candle.volume).toBe('number');

        // Basic sanity checks
        expect(candle.high).toBeGreaterThanOrEqual(candle.low);
        expect(candle.high).toBeGreaterThanOrEqual(candle.open);
        expect(candle.high).toBeGreaterThanOrEqual(candle.close);
        expect(candle.low).toBeLessThanOrEqual(candle.open);
        expect(candle.low).toBeLessThanOrEqual(candle.close);
      }

      console.log(`✅ Fetched ${candles.length} candles for XBTUSD`);
    }, 30000);

    it('should handle different timeframes', async () => {
      const intervals = ['1m', '5m', '1h', '1d'] as const;

      for (const interval of intervals) {
        const candles = await exchange.fetchOHLCV({
          symbol: 'XBTUSD',
          interval,
          limit: 2,
        });

        expect(candles).toBeDefined();
        expect(Array.isArray(candles)).toBe(true);
        console.log(`✅ Fetched ${candles.length} candles for interval ${interval}`);
      }
    }, 60000);
  });

  describe('listenOHLCV', () => {
    it.skip('should set up polling mechanism and receive at least one update', async () => {
      return new Promise<void>((resolve, reject) => {
        let updateCount = 0;

        const unsubscribe = exchange.listenOHLCV(
          {
            symbol: 'XBTUSD',
            interval: '1m',
          },
          (candle) => {
            try {
              updateCount++;
              console.log(`📊 Received candle update #${updateCount}:`, {
                timestamp: candle.timestamp,
                close: candle.close,
                volume: candle.volume,
              });

              // Validate candle structure
              expect(candle).toHaveProperty('timestamp');
              expect(candle).toHaveProperty('open');
              expect(candle).toHaveProperty('high');
              expect(candle).toHaveProperty('low');
              expect(candle).toHaveProperty('close');
              expect(candle).toHaveProperty('volume');

              // Success after first update
              unsubscribe();
              console.log(`✅ listenOHLCV working correctly`);
              resolve();
            } catch (err) {
              unsubscribe();
              reject(err);
            }
          },
        );

        // Timeout after 15 seconds (should get at least initial update)
        setTimeout(() => {
          unsubscribe();
          if (updateCount === 0) {
            reject(new Error('No OHLCV updates received within timeout'));
          } else {
            console.log(`✅ Received ${updateCount} update(s) before timeout`);
            resolve();
          }
        }, 15000);
      });
    }, 20000);
  });
});
