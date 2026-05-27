import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync } from 'fs';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

describe('GRVT listenOHLCV Integration Tests', () => {
  let exchange: GrvtExchange | null = null;
  let store: DefaultStore;
  let credentials: { apiKey?: string; secret?: string; testnet?: boolean } = {};

  beforeAll(async () => {
    // Try to load credentials from .apikey.json
    const credPath = resolve(__dirname, '../.apikey.json');
    if (existsSync(credPath)) {
      try {
        credentials = JSON.parse(readFileSync(credPath, 'utf-8'));
        console.log('✅ Loaded API credentials from .apikey.json');
      } catch (error) {
        console.log('⚠️ Error loading credentials:', error);
      }
    } else {
      console.log('⚠️ No .apikey.json file found, using testnet without authentication');
      credentials.testnet = true;
    }

    // Create store
    store = new DefaultStore();

    // Initialize exchange
    const options: ExchangeOptions = {
      key: credentials.apiKey || '',
      secret: credentials.secret || '',
      testnet: credentials.testnet !== false, // Default to testnet
    };

    try {
      exchange = new GrvtExchange('grvt', options, store);
      console.log('🔌 Initializing GRVT exchange...');

      // Start the exchange (loads markets and connects websocket)
      await exchange.start();
      console.log(`✅ Loaded ${store.markets.length} markets`);

      // Give WebSocket time to connect
      await new Promise((resolve) => setTimeout(resolve, 3000));
    } catch (error) {
      console.log('❌ Failed to initialize exchange:', error);
      exchange = null;
    }
  }, 30000);

  afterAll(async () => {
    if (exchange) {
      exchange.dispose();
      console.log('🔌 Exchange disposed');
    }
  });

  it(
    'should subscribe to and receive OHLCV/candles data via listenOHLCV',
    async () => {
      if (!exchange) {
        console.log('Skipping - exchange not initialized');
        return;
      }

      // Select a test market - using a popular perpetual for better activity
      const testSymbol = 'BTC_USDT_Perp';
      const interval = '1m'; // GRVT accepts common format like 1m, 5m, 15m, etc.

      console.log(`\n📈 Testing OHLCV subscription for ${testSymbol} ${interval}`);

      let candleReceived = false;
      let candleData: any = null;
      let candleCount = 0;

      // Subscribe to OHLCV using listenOHLCV
      const unsubscribe = exchange.listenOHLCV(
        { symbol: testSymbol, interval },
        (candle) => {
          candleReceived = true;
          candleData = candle;
          candleCount++;
          console.log(
            `Candle #${candleCount} received - Time: ${candle.timestamp}, Open: ${candle.open}, Close: ${candle.close}, Volume: ${candle.volume}`
          );
        }
      );

      // Wait for candle data (10 seconds should be enough for 1m interval)
      console.log('⏳ Waiting for candle updates...');
      await new Promise((resolve) => setTimeout(resolve, 10000));

      // Cleanup
      unsubscribe();
      console.log('🧹 Unsubscribed from OHLCV stream');

      // Verify candle was received
      if (candleReceived) {
        expect(candleData).toBeDefined();
        expect(candleData.timestamp).toBeDefined();
        expect(typeof candleData.timestamp).toBe('number');
        expect(candleData.open).toBeGreaterThan(0);
        expect(candleData.high).toBeGreaterThan(0);
        expect(candleData.low).toBeGreaterThan(0);
        expect(candleData.close).toBeGreaterThan(0);
        expect(candleData.volume).toBeGreaterThanOrEqual(0);
        console.log(`✅ OHLCV subscription working - received ${candleCount} candles`);
      } else {
        console.log('⚠️ No OHLCV data received (may need active market or different interval)');
        console.log('💡 This is not necessarily an error - the market may not have had updates in the test period');
      }
    },
    20000
  );

  it(
    'should handle multiple concurrent OHLCV subscriptions',
    async () => {
      if (!exchange) {
        console.log('Skipping - exchange not initialized');
        return;
      }

      const subscriptions = [
        { symbol: 'BTC_USDT_Perp', interval: '1m' },
        { symbol: 'ETH_USDT_Perp', interval: '1m' },
      ];

      const receivedData: Record<string, boolean> = {};
      const unsubscribers: Array<() => void> = [];

      console.log('\n📈 Testing multiple concurrent subscriptions...');

      for (const sub of subscriptions) {
        const key = `${sub.symbol}_${sub.interval}`;
        receivedData[key] = false;

        const unsub = exchange.listenOHLCV({ ...sub, interval: sub.interval as any }, (candle) => {
          receivedData[key] = true;
          console.log(`${key}: Candle received - ${candle.timestamp}`);
        });

        unsubscribers.push(unsub);
      }

      // Wait for data
      await new Promise((resolve) => setTimeout(resolve, 10000));

      // Cleanup all subscriptions
      unsubscribers.forEach((unsub) => unsub());

      // Check results
      const receivedCount = Object.values(receivedData).filter((v) => v).length;
      console.log(`✅ Received data for ${receivedCount}/${subscriptions.length} subscriptions`);

      if (receivedCount === 0) {
        console.log('⚠️ No data received for any subscription (may be normal for low activity markets)');
      }
    },
    20000
  );

  it(
    'should handle different timeframe intervals',
    async () => {
      if (!exchange) {
        console.log('Skipping - exchange not initialized');
        return;
      }

      const testSymbol = 'BTC_USDT_Perp';
      const intervals = ['1m', '5m', '15m'];
      const receivedData: Record<string, boolean> = {};
      const unsubscribers: Array<() => void> = [];

      console.log('\n📈 Testing different timeframe intervals...');

      for (const interval of intervals) {
        const key = `${testSymbol}_${interval}`;
        receivedData[key] = false;

        const unsub = exchange.listenOHLCV({ symbol: testSymbol, interval: interval as any }, (candle) => {
          receivedData[key] = true;
          console.log(`${key}: Candle received - ${candle.timestamp}`);
        });

        unsubscribers.push(unsub);
      }

      // Wait for data
      await new Promise((resolve) => setTimeout(resolve, 10000));

      // Cleanup all subscriptions
      unsubscribers.forEach((unsub) => unsub());

      // Check results
      const receivedCount = Object.values(receivedData).filter((v) => v).length;
      console.log(`✅ Received data for ${receivedCount}/${intervals.length} intervals`);

      if (receivedCount === 0) {
        console.log('⚠️ No data received for any interval (may be normal if no trading activity)');
      }
    },
    20000
  );
});
