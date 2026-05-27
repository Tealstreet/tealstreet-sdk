/**
 * @vitest-environment node
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync } from 'fs';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

/**
 * Complete integration test for GRVT exchange
 * Verifies all data is loaded and store.loaded flags are properly set
 */
describe('GRVT Complete Integration Test', () => {
  let exchange: GrvtExchange | null = null;
  let store: DefaultStore;
  let credentials: { apiKey?: string; secret?: string; testnet?: boolean; accountId?: string } = {};

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
      console.log('⚠️ No .apikey.json file found');
      console.log('💡 Create a .apikey.json file with your GRVT credentials to run this test');
      return;
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
      console.log('\n🔌 Initializing GRVT exchange...');

      // Start the exchange (this should load all initial data)
      await exchange.start();
      console.log('✅ Exchange started successfully');

      // Give some time for async operations to complete
      await new Promise((resolve) => setTimeout(resolve, 2000));
    } catch (error) {
      console.log('❌ Failed to initialize exchange:', error);
      exchange = null;
    }
  }, 60000);

  afterAll(async () => {
    if (exchange) {
      exchange.dispose();
      console.log('\n🔌 Exchange disposed');
    }
  });

  it('should have valid credentials', () => {
    if (!credentials.apiKey || !credentials.secret) {
      console.log('⚠️ Skipping - no credentials found');
      return;
    }

    expect(credentials.apiKey).toBeDefined();
    expect(credentials.secret).toBeDefined();
    expect(typeof credentials.apiKey).toBe('string');
    expect(typeof credentials.secret).toBe('string');
  });

  it('should initialize exchange successfully', () => {
    if (!credentials.apiKey || !credentials.secret) {
      console.log('⚠️ Skipping - no credentials found');
      return;
    }

    expect(exchange).toBeDefined();
    expect(exchange).not.toBeNull();
  });

  describe('Store Data Loading', () => {
    it('should load markets data', () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n📊 Markets Status:');
      console.log(`  Total markets: ${store.markets.length}`);
      console.log(`  Loaded flag: ${store.loaded.markets}`);

      // Verify markets are loaded
      expect(store.markets).toBeDefined();
      expect(Array.isArray(store.markets)).toBe(true);
      expect(store.markets.length).toBeGreaterThan(0);

      // Verify market structure
      const firstMarket = store.markets[0];
      expect(firstMarket).toHaveProperty('id');
      expect(firstMarket).toHaveProperty('symbol');
      expect(firstMarket).toHaveProperty('base');
      expect(firstMarket).toHaveProperty('quote');
      expect(firstMarket).toHaveProperty('precision');
      expect(firstMarket).toHaveProperty('limits');

      // Check loaded flag
      if (!store.loaded.markets) {
        console.log('❌ ISSUE: markets loaded flag is FALSE - needs to be set in start() method');
      } else {
        console.log('✅ Markets loaded flag is properly set');
      }
      expect(store.loaded.markets).toBe(true);
    });

    it('should load tickers data', () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n📈 Tickers Status:');
      console.log(`  Total tickers: ${store.tickers.length}`);
      console.log(`  Loaded flag: ${store.loaded.tickers}`);

      // Check loaded flag
      if (!store.loaded.tickers) {
        console.log('❌ ISSUE: tickers loaded flag is FALSE - needs to be set in start() method');
        console.log('💡 Note: GRVT may not have tickers available via REST API');
        console.log('💡 Consider implementing via WebSocket or marking as not applicable');
      } else {
        console.log('✅ Tickers loaded flag is properly set');
      }

      // Tickers might be empty if loaded via WebSocket later
      expect(store.tickers).toBeDefined();
      expect(Array.isArray(store.tickers)).toBe(true);
      expect(store.loaded.tickers).toBe(true);
    });

    it('should load balance data', async () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n💰 Balance Status:');
      console.log(`  Total balance: ${store.balance?.total ?? 'N/A'}`);
      console.log(`  Free balance: ${store.balance?.free ?? 'N/A'}`);
      console.log(`  Used balance: ${store.balance?.used ?? 'N/A'}`);
      console.log(`  Loaded flag: ${store.loaded.balance}`);

      if (!store.loaded.balance) {
        console.log('❌ ISSUE: balance loaded flag is FALSE');
        console.log('💡 SOLUTION: Call fetchBalance() in start() method and set loaded flag');

        // Try to fetch balance manually to verify it works
        try {
          const balance = await exchange.fetchBalance();
          console.log('✅ fetchBalance() works correctly:');
          console.log(`   Total: ${balance.total}, Free: ${balance.free}, Used: ${balance.used}`);
        } catch (error) {
          console.log('❌ fetchBalance() failed:', error);
        }
      } else {
        console.log('✅ Balance loaded flag is properly set');
        expect(store.balance).toBeDefined();
        expect(typeof store.balance.total).toBe('number');
        expect(typeof store.balance.free).toBe('number');
        expect(typeof store.balance.used).toBe('number');
      }

      expect(store.loaded.balance).toBe(true);
    });

    it('should load positions data', async () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n📍 Positions Status:');
      console.log(`  Total positions: ${store.positions.length}`);
      console.log(`  Loaded flag: ${store.loaded.positions}`);

      if (!store.loaded.positions) {
        console.log('❌ ISSUE: positions loaded flag is FALSE');
        console.log('💡 SOLUTION: Call fetchPositions() in start() method and set loaded flag');

        // Try to fetch positions manually to verify it works
        try {
          const positions = await exchange.fetchPositions();
          console.log(`✅ fetchPositions() works correctly - found ${positions.length} position(s)`);
          if (positions.length > 0) {
            positions.forEach((pos, idx) => {
              console.log(`   Position ${idx + 1}: ${pos.symbol} ${pos.side} ${pos.contracts} @ ${pos.entryPrice}`);
            });
          }
        } catch (error) {
          console.log('❌ fetchPositions() failed:', error);
        }
      } else {
        console.log('✅ Positions loaded flag is properly set');
        expect(store.positions).toBeDefined();
        expect(Array.isArray(store.positions)).toBe(true);
      }

      expect(store.loaded.positions).toBe(true);
    });

    it('should load orders data', async () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n📋 Orders Status:');
      console.log(`  Total orders: ${store.orders.length}`);
      console.log(`  Loaded flag: ${store.loaded.orders}`);

      if (!store.loaded.orders) {
        console.log('❌ ISSUE: orders loaded flag is FALSE');
        console.log('💡 SOLUTION: Call fetchOpenOrders() in start() method and set loaded flag');

        // Try to fetch orders manually to verify it works
        try {
          const orders = await exchange.fetchOpenOrders();
          console.log(`✅ fetchOpenOrders() works correctly - found ${orders.length} order(s)`);
          if (orders.length > 0) {
            orders.forEach((order, idx) => {
              console.log(`   Order ${idx + 1}: ${order.symbol} ${order.side} ${order.amount} @ ${order.price}`);
            });
          }
        } catch (error) {
          console.log('❌ fetchOpenOrders() failed:', error);
        }
      } else {
        console.log('✅ Orders loaded flag is properly set');
        expect(store.orders).toBeDefined();
        expect(Array.isArray(store.orders)).toBe(true);
      }

      expect(store.loaded.orders).toBe(true);
    });
  });

  describe('Store Loaded Summary', () => {
    it('should have all loaded flags set to true', () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n' + '='.repeat(50));
      console.log('📊 STORE LOADED FLAGS SUMMARY');
      console.log('='.repeat(50));
      console.log(`Markets:   ${store.loaded.markets ? '✅ LOADED' : '❌ NOT LOADED'}`);
      console.log(`Tickers:   ${store.loaded.tickers ? '✅ LOADED' : '❌ NOT LOADED'}`);
      console.log(`Balance:   ${store.loaded.balance ? '✅ LOADED' : '❌ NOT LOADED'}`);
      console.log(`Positions: ${store.loaded.positions ? '✅ LOADED' : '❌ NOT LOADED'}`);
      console.log(`Orders:    ${store.loaded.orders ? '✅ LOADED' : '❌ NOT LOADED'}`);
      console.log('='.repeat(50));

      const allLoaded =
        store.loaded.markets &&
        store.loaded.tickers &&
        store.loaded.balance &&
        store.loaded.positions &&
        store.loaded.orders;

      if (!allLoaded) {
        console.log('\n⚠️ NOT ALL DATA IS LOADED - See individual test results above for fixes');
      } else {
        console.log('\n✅ ALL DATA LOADED SUCCESSFULLY');
      }

      // These should all be true for a complete implementation
      expect(store.loaded.markets).toBe(true);
      expect(store.loaded.tickers).toBe(true);
      expect(store.loaded.balance).toBe(true);
      expect(store.loaded.positions).toBe(true);
      expect(store.loaded.orders).toBe(true);
    });
  });

  describe('Data Integrity', () => {
    it('should have consistent market and ticker data', () => {
      if (!exchange) {
        console.log('⚠️ Skipping - exchange not initialized');
        return;
      }

      console.log('\n🔍 Data Integrity Check:');

      // If we have tickers, they should match markets
      if (store.tickers.length > 0) {
        const tickerSymbols = new Set(store.tickers.map((t) => t.symbol));
        const marketSymbols = new Set(store.markets.map((m) => m.symbol));

        const allTickersHaveMarkets = store.tickers.every((t) => marketSymbols.has(t.symbol));
        console.log(`  All tickers have corresponding markets: ${allTickersHaveMarkets ? '✅' : '❌'}`);

        if (!allTickersHaveMarkets) {
          const orphanTickers = store.tickers.filter((t) => !marketSymbols.has(t.symbol));
          console.log(`  Orphan tickers (no market): ${orphanTickers.map((t) => t.symbol).join(', ')}`);
        }
      } else {
        console.log('  No tickers loaded yet (may be populated via WebSocket)');
      }

      // Check positions reference valid markets
      if (store.positions.length > 0) {
        const marketSymbols = new Set(store.markets.map((m) => m.symbol));
        const allPositionsHaveMarkets = store.positions.every((p) => marketSymbols.has(p.symbol));
        console.log(`  All positions reference valid markets: ${allPositionsHaveMarkets ? '✅' : '❌'}`);
      } else {
        console.log('  No open positions');
      }

      // Check orders reference valid markets
      if (store.orders.length > 0) {
        const marketSymbols = new Set(store.markets.map((m) => m.symbol));
        const allOrdersHaveMarkets = store.orders.every((o) => marketSymbols.has(o.symbol));
        console.log(`  All orders reference valid markets: ${allOrdersHaveMarkets ? '✅' : '❌'}`);
      } else {
        console.log('  No open orders');
      }
    });

    it('should have valid balance data structure', () => {
      if (!exchange || !store.loaded.balance) {
        console.log('⚠️ Skipping - balance not loaded');
        return;
      }

      expect(store.balance).toBeDefined();
      expect(store.balance).toHaveProperty('total');
      expect(store.balance).toHaveProperty('free');
      expect(store.balance).toHaveProperty('used');
      expect(store.balance).toHaveProperty('upnl');

      expect(typeof store.balance.total).toBe('number');
      expect(typeof store.balance.free).toBe('number');
      expect(typeof store.balance.used).toBe('number');
      expect(typeof store.balance.upnl).toBe('number');

      // Verify balance equation: total = free + used (approximately, accounting for floating point)
      const calculatedTotal = store.balance.free + store.balance.used;
      const diff = Math.abs(store.balance.total - calculatedTotal);
      console.log(`\n  Balance equation check:`);
      console.log(`    Total: ${store.balance.total}`);
      console.log(`    Free + Used: ${calculatedTotal}`);
      console.log(`    Difference: ${diff}`);

      // Allow small floating point differences
      expect(diff).toBeLessThan(0.01);
    });
  });
});
