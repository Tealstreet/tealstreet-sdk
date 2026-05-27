#!/usr/bin/env tsx

/**
 * Simple test to verify Arkham orderbook subscription is working
 */

import path from 'path';
import fs from 'fs';
import { ArkhamExchange } from '../arkham.exchange';
import type { ExchangeOptions } from '../../../types';
import { DefaultStore } from '../../../store/store.base';

const credentialsPath = path.join(__dirname, '..', '.apikey.json');
let credentials: any = {};

if (fs.existsSync(credentialsPath)) {
  credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf8'));
}

const options: ExchangeOptions = {
  key: credentials.apiKey || '',
  secret: credentials.secret || credentials.apiSecret || '',
  testnet: credentials.testnet ?? false,
};

// Create store and exchange instance
const store = new DefaultStore();
const exchange = new ArkhamExchange('arkham', options, store);

async function testOrderbook() {
  console.log('🔌 Testing Arkham Orderbook Subscription');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    // Start exchange without validation to avoid auth issues
    console.log('📡 Starting exchange...');
    await exchange.start();

    // Wait for WebSocket connection
    await new Promise(resolve => setTimeout(resolve, 3000));

    const publicWs = (exchange as any).publicWs;
    if (!publicWs) {
      console.error('❌ Public WebSocket not initialized');
      process.exit(1);
    }

    console.log('✅ WebSocket connected:', publicWs.isConnected);
    console.log('📊 Markets loaded:', exchange.store.markets.length);

    // Test BTC_PERP orderbook subscription
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 Subscribing to BTC_PERP orderbook...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    let orderbookCount = 0;

    const unsubscribe = exchange.listenOrderBook('BTC_PERP', (orderbook) => {
      orderbookCount++;
      console.log(`📊 Orderbook update #${orderbookCount}:`);
      console.log(`   Symbol: BTC_PERP`);
      console.log(`   Bids: ${orderbook.bids.length}`);
      console.log(`   Asks: ${orderbook.asks.length}`);
      if (orderbook.bids.length > 0 && orderbook.asks.length > 0) {
        console.log(`   Best Bid: ${orderbook.bids[0]?.price}`);
        console.log(`   Best Ask: ${orderbook.asks[0]?.price}`);
        console.log(`   Spread: ${(orderbook.asks[0].price - orderbook.bids[0].price).toFixed(2)}`);
      }
      console.log('');
    });

    // Wait 15 seconds for updates
    console.log('⏳ Waiting 15 seconds for orderbook updates...\n');
    await new Promise(resolve => setTimeout(resolve, 15000));

    // Report results
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 Test Results');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    if (orderbookCount > 0) {
      console.log(`✅ SUCCESS! Received ${orderbookCount} orderbook updates`);
      console.log('✅ Arkham orderbook subscription is working correctly');
    } else {
      console.log('❌ FAILED: No orderbook updates received');
      console.log('\nDebugging info:');
      const handlers = (publicWs as any).messageHandlers;
      console.log('  Registered handlers:', Object.keys(handlers));
      console.log('  WebSocket connected:', publicWs.isConnected);
    }

    // Cleanup
    console.log('\n🧹 Cleaning up...');
    unsubscribe();
    exchange.dispose();

    process.exit(orderbookCount > 0 ? 0 : 1);

  } catch (error) {
    console.error('❌ Test failed with error:', error);
    exchange.dispose();
    process.exit(1);
  }
}

// Run the test
testOrderbook().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
