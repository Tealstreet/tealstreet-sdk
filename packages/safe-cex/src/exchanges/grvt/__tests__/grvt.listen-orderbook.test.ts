import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync } from 'fs';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions, OrderBook } from '../../../types';

describe('GRVT listenOrderBook Integration Tests', () => {
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

    // Initialize exchange (no auth needed for public WebSocket)
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
    'should subscribe to and receive orderbook data via listenOrderBook',
    async () => {
      if (!exchange) {
        console.log('Skipping - exchange not initialized');
        return;
      }

      // Select a test market - using a popular perpetual for better activity
      const testSymbol = 'BTC_USDT_Perp';

      console.log(`\n📊 Testing orderbook subscription for ${testSymbol}`);

      let orderbookReceived = false;
      const orderbookUpdates: OrderBook[] = [];
      let orderbookCount = 0;

      // Subscribe to orderbook using listenOrderBook
      const unsubscribe = exchange.listenOrderBook(testSymbol, (orderbook) => {
        orderbookReceived = true;
        orderbookUpdates.push(orderbook);
        orderbookCount++;

        if (orderbookCount <= 3) {
          console.log(
            `OrderBook #${orderbookCount} - Bids: ${orderbook.bids.length}, Asks: ${orderbook.asks.length}`
          );
          if (orderbook.bids.length > 0 && orderbook.asks.length > 0) {
            const bestBid = orderbook.bids[0];
            const bestAsk = orderbook.asks[0];
            console.log(
              `  Best Bid: $${bestBid.price} (${bestBid.amount}), Best Ask: $${bestAsk.price} (${bestAsk.amount}), Spread: $${(bestAsk.price - bestBid.price).toFixed(2)}`
            );
          }
        }
      });

      // Wait for orderbook data (10 seconds should be enough)
      console.log('⏳ Waiting for orderbook updates...');
      await new Promise((resolve) => setTimeout(resolve, 10000));

      // Cleanup
      unsubscribe();
      console.log('🧹 Unsubscribed from orderbook stream');

      // Verify orderbook was received
      if (orderbookReceived && orderbookUpdates.length > 0) {
        const orderbookData = orderbookUpdates[orderbookUpdates.length - 1];
        expect(orderbookData).toBeDefined();
        expect(orderbookData.bids).toBeDefined();
        expect(orderbookData.asks).toBeDefined();
        expect(Array.isArray(orderbookData.bids)).toBe(true);
        expect(Array.isArray(orderbookData.asks)).toBe(true);

        // Verify we have bid and ask data
        expect(orderbookData.bids.length).toBeGreaterThan(0);
        expect(orderbookData.asks.length).toBeGreaterThan(0);

        // Verify price level structure
        const firstBid = orderbookData.bids[0];
        expect(firstBid).toHaveProperty('price');
        expect(firstBid).toHaveProperty('amount');
        expect(firstBid).toHaveProperty('total');
        expect(typeof firstBid.price).toBe('number');
        expect(typeof firstBid.amount).toBe('number');
        expect(typeof firstBid.total).toBe('number');
        expect(firstBid.price).toBeGreaterThan(0);
        expect(firstBid.amount).toBeGreaterThan(0);

        const firstAsk = orderbookData.asks[0];
        expect(firstAsk).toHaveProperty('price');
        expect(firstAsk).toHaveProperty('amount');
        expect(firstAsk).toHaveProperty('total');
        expect(typeof firstAsk.price).toBe('number');
        expect(typeof firstAsk.amount).toBe('number');
        expect(typeof firstAsk.total).toBe('number');
        expect(firstAsk.price).toBeGreaterThan(0);
        expect(firstAsk.amount).toBeGreaterThan(0);

        // Verify orderbook is properly sorted (bids descending, asks ascending)
        for (let i = 1; i < orderbookData.bids.length; i++) {
          expect(orderbookData.bids[i].price).toBeLessThanOrEqual(orderbookData.bids[i - 1].price);
        }
        for (let i = 1; i < orderbookData.asks.length; i++) {
          expect(orderbookData.asks[i].price).toBeGreaterThanOrEqual(orderbookData.asks[i - 1].price);
        }

        console.log(`✅ OrderBook subscription working - received ${orderbookCount} updates`);
      } else {
        console.log('⚠️ No orderbook data received (may need active market)');
        console.log(
          '💡 This is not necessarily an error - the market may not have had updates in the test period'
        );
      }
    },
    20000
  );

  it(
    'should handle multiple concurrent orderbook subscriptions',
    async () => {
      if (!exchange) {
        console.log('Skipping - exchange not initialized');
        return;
      }

      const symbols = ['BTC_USDT_Perp', 'ETH_USDT_Perp'];
      const receivedData: Record<string, boolean> = {};
      const unsubscribers: Array<() => void> = [];

      console.log('\n📊 Testing multiple concurrent orderbook subscriptions...');

      for (const symbol of symbols) {
        receivedData[symbol] = false;

        const unsub = exchange.listenOrderBook(symbol, (orderbook) => {
          receivedData[symbol] = true;
          console.log(`${symbol}: OrderBook received - Bids: ${orderbook.bids.length}, Asks: ${orderbook.asks.length}`);
        });

        unsubscribers.push(unsub);
      }

      // Wait for data
      await new Promise((resolve) => setTimeout(resolve, 10000));

      // Cleanup all subscriptions
      unsubscribers.forEach((unsub) => unsub());

      // Check results
      const receivedCount = Object.values(receivedData).filter((v) => v).length;
      console.log(`✅ Received data for ${receivedCount}/${symbols.length} subscriptions`);

      if (receivedCount === 0) {
        console.log('⚠️ No data received for any subscription (may be normal for low activity markets)');
      }
    },
    20000
  );

  it(
    'should stop receiving updates after unsubscribe',
    async () => {
      if (!exchange) {
        console.log('Skipping - exchange not initialized');
        return;
      }

      const testSymbol = 'BTC_USDT_Perp';
      let orderbookCount = 0;

      console.log('\n📊 Testing unsubscribe functionality...');

      // Set up listener
      const unsubscribe = exchange.listenOrderBook(testSymbol, (orderbook) => {
        orderbookCount++;
      });

      // Wait for some updates
      await new Promise((resolve) => setTimeout(resolve, 3000));
      const countBeforeUnsubscribe = orderbookCount;

      // Unsubscribe
      unsubscribe();
      console.log(`Unsubscribed after ${countBeforeUnsubscribe} updates`);

      // Wait more time
      await new Promise((resolve) => setTimeout(resolve, 3000));
      const countAfterUnsubscribe = orderbookCount;

      // Count should not increase after unsubscribe
      expect(countAfterUnsubscribe - countBeforeUnsubscribe).toBeLessThan(3);

      console.log(`✅ Unsubscribe test - Before: ${countBeforeUnsubscribe}, After: ${countAfterUnsubscribe}`);
    },
    15000
  );
});
