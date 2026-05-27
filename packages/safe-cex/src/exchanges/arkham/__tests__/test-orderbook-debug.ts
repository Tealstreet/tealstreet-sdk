#!/usr/bin/env tsx

/**
 * Debug test for Arkham orderbook - investigating channel name issue
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

async function testOrderbookDebug() {
  console.log('🔍 Arkham Orderbook Debug Test');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    // Start exchange
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

    // Add comprehensive debug logging
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('🔍 Adding debug logging to WebSocket...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Store original onMessage
    const originalOnMessage = publicWs.onMessage.bind(publicWs);

    // Track all messages received
    const messageStats = new Map<string, number>();

    // Override onMessage to add logging
    publicWs.onMessage = (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);

        // Track message types
        const msgType = data.channel || data.type || 'unknown';
        messageStats.set(msgType, (messageStats.get(msgType) || 0) + 1);

        // Log ALL messages to see what we're actually receiving
        console.log('📨 Message received:', {
          type: data.type,
          channel: data.channel,
          method: data.method,
          hasData: !!data.data,
          dataKeys: data.data ? Object.keys(data.data) : [],
          timestamp: new Date().toISOString(),
        });

        // Special logging for orderbook-related messages
        if (data.channel && (data.channel.includes('l2') || data.channel.includes('book') || data.channel.includes('depth'))) {
          console.log('📊 ORDERBOOK MESSAGE DETECTED!');
          console.log('   Full message:', JSON.stringify(data, null, 2));
        }

        // Call original handler
        originalOnMessage(event);
      } catch (err) {
        console.error('❌ Error parsing message:', err);
        originalOnMessage(event);
      }
    };

    // Override send to see what we're sending
    const originalSend = publicWs.send.bind(publicWs);
    publicWs.send = (data: string) => {
      try {
        const parsed = JSON.parse(data);
        console.log('📤 Sending message:', {
          method: parsed.method,
          channel: parsed.args?.channel,
          params: parsed.args?.params,
        });
      } catch {}
      originalSend(data);
    };

    // Check registered handlers
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📋 Registered message handlers:');
    const handlers = (publicWs as any).messageHandlers;
    console.log(Object.keys(handlers));
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Subscribe to BTC_PERP orderbook
    console.log('📊 Subscribing to BTC_PERP orderbook via listenOrderBook...');
    let orderbookCallbackCount = 0;

    const unsubscribe = exchange.listenOrderBook('BTC_PERP', (orderbook) => {
      orderbookCallbackCount++;
      console.log(`✅ ORDERBOOK CALLBACK TRIGGERED! (count: ${orderbookCallbackCount})`);
      console.log('   Bids:', orderbook.bids.length);
      console.log('   Asks:', orderbook.asks.length);
      if (orderbook.bids[0]) console.log('   Best Bid:', orderbook.bids[0].price);
      if (orderbook.asks[0]) console.log('   Best Ask:', orderbook.asks[0].price);
      console.log('');
    });

    // Wait and observe
    console.log('\n⏳ Waiting 20 seconds to observe messages...\n');

    for (let i = 0; i < 20; i++) {
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Every 5 seconds, show stats
      if (i % 5 === 0 && i > 0) {
        console.log(`\n⏱️  ${i}s elapsed`);
        console.log('📊 Message statistics:');
        messageStats.forEach((count, type) => {
          console.log(`   ${type}: ${count}`);
        });
        console.log(`   Orderbook callbacks: ${orderbookCallbackCount}\n`);
      }
    }

    // Final report
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 Final Results');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log('Total message types received:');
    messageStats.forEach((count, type) => {
      console.log(`  ${type}: ${count}`);
    });
    console.log(`\nOrderbook callbacks: ${orderbookCallbackCount}`);

    if (orderbookCallbackCount === 0) {
      console.log('\n❌ ISSUE IDENTIFIED:');
      console.log('No orderbook callbacks were triggered.');
      console.log('Check the message log above to see what channels were actually received.');
      console.log('\nPossible causes:');
      console.log('1. Channel name mismatch (l2_updates vs l2_book vs orderbook)');
      console.log('2. Subscription message format incorrect');
      console.log('3. Symbol format issue');
      console.log('4. Data parsing issue in handler');
    }

    // Cleanup
    console.log('\n🧹 Cleaning up...');
    unsubscribe();
    exchange.dispose();

    process.exit(orderbookCallbackCount > 0 ? 0 : 1);

  } catch (error) {
    console.error('❌ Test failed with error:', error);
    exchange.dispose();
    process.exit(1);
  }
}

// Run the test
testOrderbookDebug().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
