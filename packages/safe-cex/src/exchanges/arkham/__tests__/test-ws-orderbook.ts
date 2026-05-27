#!/usr/bin/env tsx

/**
 * Comprehensive test for Arkham orderbook subscription via WebSocket
 *
 * This test validates that:
 * 1. Orderbook subscription messages are being sent correctly
 * 2. WebSocket is receiving orderbook updates
 * 3. listenOrderBook callback is triggered with real-time updates
 * 4. Message routing and parsing work correctly
 */

import path from 'path';
import fs from 'fs';
import { ArkhamExchange } from '../arkham.exchange';
import type { ExchangeOptions } from '../../../types';
import { OrderBook } from '../../../types';
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

async function testOrderbookSubscription() {
  console.log('🔌 Testing Arkham Orderbook Subscription');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    // Step 1: Validate account and start exchange
    console.log('📡 Starting exchange and connecting to WebSocket...');
    const isValid = await exchange.validateAccount();
    if (!isValid) {
      console.error('❌ Account validation failed');
      process.exit(1);
    }

    await exchange.start();

    // Wait for initial connection
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Step 2: Debug WebSocket connection state
    const publicWs = (exchange as any).publicWs;
    if (!publicWs) {
      console.error('❌ Public WebSocket not initialized');
      process.exit(1);
    }

    console.log('✅ WebSocket connected:', publicWs.isConnected);
    console.log('📊 Markets loaded:', exchange.store.markets.length);
    console.log('💰 Tickers loaded:', exchange.store.tickers.length);

    // Step 3: Add debug logging to WebSocket
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('🔍 Adding debug logging to WebSocket...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Store original onMessage for later
    const originalOnMessage = publicWs.onMessage.bind(publicWs);

    // Override onMessage to add logging
    publicWs.onMessage = (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);

        // Log every message received
        if (data.channel === 'l2_book') {
          console.log('📨 Orderbook message received:', {
            channel: data.channel,
            symbol: data.data?.symbol,
            timestamp: new Date().toISOString(),
            hasData: !!data.data,
            bidCount: data.data?.bids?.length || 0,
            askCount: data.data?.asks?.length || 0,
          });
        } else if (data.channel) {
          console.log(`📨 Other message (${data.channel}):`, data.channel === 'ticker' ? 'ticker update' : JSON.stringify(data).substring(0, 100));
        }

        // Call original handler
        originalOnMessage(event);
      } catch (err) {
        console.error('Error parsing message:', err);
        originalOnMessage(event);
      }
    };

    // Also override the send method to log outgoing messages
    const originalSend = publicWs.send.bind(publicWs);
    publicWs.send = (data: string) => {
      try {
        const parsed = JSON.parse(data);
        if (parsed.args?.channel === 'l2_book') {
          console.log('📤 Sending orderbook subscription:', parsed);
        }
      } catch {}
      originalSend(data);
    };

    // Step 4: Subscribe to BTC_PERP orderbook
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 Subscribing to BTC_PERP orderbook...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    let orderbookCount = 0;
    let firstOrderbook: OrderBook | null = null;
    let lastOrderbook: OrderBook | null = null;

    // Subscribe using listenOrderBook
    const unsubscribe = exchange.listenOrderBook('BTC_PERP', (orderbook) => {
      orderbookCount++;

      if (!firstOrderbook) {
        firstOrderbook = orderbook;
        console.log('✅ FIRST ORDERBOOK RECEIVED!');
        console.log('   Symbol: BTC_PERP');
        console.log('   Bids:', orderbook.bids.length);
        console.log('   Asks:', orderbook.asks.length);
        console.log('   Best Bid:', orderbook.bids[0]?.price);
        console.log('   Best Ask:', orderbook.asks[0]?.price);
        console.log('   Timestamp:', new Date().toISOString());
        console.log('');
      }

      lastOrderbook = orderbook;

      // Log every 10th update
      if (orderbookCount % 10 === 0) {
        console.log(`📊 Update #${orderbookCount}: Bids=${orderbook.bids.length}, Asks=${orderbook.asks.length}, Best Bid=${orderbook.bids[0]?.price}, Best Ask=${orderbook.asks[0]?.price}`);
      }
    });

    // Also try manual subscription to see if that works
    console.log('📤 Sending manual subscription...');
    publicWs.subscribeToOrderBook('BTC_PERP');

    // Step 5: Wait and collect data
    console.log('\n⏳ Waiting 30 seconds for orderbook updates...\n');

    // Check periodically for updates
    for (let i = 0; i < 30; i++) {
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Every 5 seconds, show status
      if (i % 5 === 0 && i > 0) {
        console.log(`⏱️  ${i}s elapsed - Received ${orderbookCount} orderbook updates`);

        // Check message handlers
        const handlers = (publicWs as any).messageHandlers;
        console.log('   Registered handlers:', Object.keys(handlers));
        if (handlers.l2_book) {
          console.log('   l2_book handlers count:', handlers.l2_book.length);
        }
      }
    }

    // Step 6: Report results
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 Test Results');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log(`Total orderbook updates received: ${orderbookCount}`);

    if (orderbookCount > 0 && firstOrderbook && lastOrderbook) {
      // Create const references to satisfy TypeScript narrowing
      const first: OrderBook = firstOrderbook;
      const last: OrderBook = lastOrderbook;

      console.log('✅ Orderbook subscription WORKING!');
      console.log('\nFirst orderbook:');
      console.log('  Bids:', first.bids.length);
      console.log('  Asks:', first.asks.length);
      console.log('  Best Bid:', first.bids[0]?.price);
      console.log('  Best Ask:', first.asks[0]?.price);

      console.log('\nLast orderbook:');
      console.log('  Bids:', last.bids.length);
      console.log('  Asks:', last.asks.length);
      console.log('  Best Bid:', last.bids[0]?.price);
      console.log('  Best Ask:', last.asks[0]?.price);

      // Check if prices changed (real-time updates)
      const bidChanged = first.bids[0]?.price !== last.bids[0]?.price;
      const askChanged = first.asks[0]?.price !== last.asks[0]?.price;

      if (bidChanged || askChanged) {
        console.log('\n✅ Real-time updates confirmed - prices changed!');
      } else {
        console.log('\n⚠️  Prices did not change during test period');
      }
    } else {
      console.log('❌ NO ORDERBOOK UPDATES RECEIVED!');
      console.log('\nPossible issues:');
      console.log('1. Subscription message format incorrect');
      console.log('2. Channel name mismatch (l2_book vs orderbook)');
      console.log('3. Symbol format issue (BTC_PERP vs BTC-PERP)');
      console.log('4. WebSocket authentication required for orderbook');
      console.log('5. Message parsing/routing issue');

      // Check if we have any message handlers registered
      const handlers = (publicWs as any).messageHandlers;
      console.log('\nDebug info:');
      console.log('  Message handlers:', Object.keys(handlers));
      console.log('  WebSocket connected:', publicWs.isConnected);
      console.log('  WebSocket disposed:', (publicWs as any).isDisposed);
    }

    // Cleanup
    console.log('\n🧹 Cleaning up...');
    unsubscribe();
    exchange.dispose();

    // Exit with appropriate code
    process.exit(orderbookCount > 0 ? 0 : 1);

  } catch (error) {
    console.error('\n❌ Test failed with error:', error);
    exchange.dispose();
    process.exit(1);
  }
}

// Run the test
testOrderbookSubscription().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
