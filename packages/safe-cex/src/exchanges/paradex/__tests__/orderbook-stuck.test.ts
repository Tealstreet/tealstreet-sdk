/**
 * Diagnostic Script for Paradex Orderbook Stuck Issue
 *
 * This script monitors orderbook updates and helps diagnose if:
 * 1. WebSocket is connected and receiving messages
 * 2. Orderbook callbacks are being triggered
 * 3. Resubscription is working after reconnect
 *
 * Run with: npx tsx packages/safe-cex/src/exchanges/paradex/test-orderbook-stuck.ts
 */

import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import * as fs from 'fs';
import * as path from 'path';

async function main() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📊 Paradex Orderbook Diagnostic');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const TEST_SYMBOL = 'ETH-USD-PERP';
  let updateCount = 0;
  let lastUpdateTime = Date.now();
  let lastBid = 0;
  let lastAsk = 0;

  // Load credentials
  const credsPath = path.join(__dirname, '.apikey.json');
  let testCreds: any = null;
  if (fs.existsSync(credsPath)) {
    testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
    console.log('✓ Loaded credentials from .apikey.json');
    console.log(`  Account: ${testCreds.paradexAddress}`);
    console.log(`  Subkey: ${testCreds.subkeyAddress}`);
    console.log(`  Testnet: ${testCreds.testnet}\n`);
  } else {
    console.log('⚠️  No .apikey.json found - running in public-only mode');
    console.log('   (Private WebSocket and authenticated endpoints will not work)\n');
  }

  // Create store and exchange
  const store = new DefaultStore();
  const exchange = new ParadexExchange(
    'paradex',
    testCreds
      ? {
          key: testCreds.apiKey,
          secret: '', // Not used for Paradex
          testnet: testCreds.testnet || false,
          paradexAddress: testCreds.paradexAddress,
          subkeyAddress: testCreds.subkeyAddress,
        }
      : { key: '', secret: '', testnet: false },
    store,
  );

  // Monitor WebSocket state
  const wsPublic = (exchange as any).publicWebsocket;
  const wsPrivate = (exchange as any).privateWebsocket;

  // Log WebSocket events
  setInterval(() => {
    const publicConnected = wsPublic.isConnected;
    const privateConnected = wsPrivate.isConnected;
    const timeSinceUpdate = Date.now() - lastUpdateTime;

    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log(`⏰ ${new Date().toLocaleTimeString()}`);
    console.log(`📡 Public WS: ${publicConnected ? '✅ Connected' : '❌ Disconnected'}`);
    console.log(`📡 Private WS: ${privateConnected ? '✅ Connected' : '❌ Disconnected'}`);
    console.log(`📊 Orderbook updates: ${updateCount}`);
    console.log(`⏱️  Time since last update: ${(timeSinceUpdate / 1000).toFixed(1)}s`);
    console.log(`💰 Last bid: ${lastBid}, ask: ${lastAsk}`);

    // Check callback registration
    const callbacks = (wsPublic as any).orderbookCallbacks;
    const hasCallback = callbacks.has(TEST_SYMBOL);
    const callbackCount = hasCallback ? callbacks.get(TEST_SYMBOL).size : 0;
    console.log(`🔔 Callbacks registered for ${TEST_SYMBOL}: ${callbackCount}`);

    // Check message handler stats
    if ((wsPublic as any).messageHandlerStats) {
      const stats = (wsPublic as any).messageHandlerStats;
      console.log('📨 Message handler stats:', stats);
    }

    // Warn if no updates for 30 seconds
    if (timeSinceUpdate > 30000 && updateCount > 0) {
      console.log('⚠️  WARNING: No orderbook updates for 30+ seconds!');
    }
  }, 5000);

  try {
    console.log('🔄 Starting Paradex exchange...\n');
    await exchange.start();
    console.log('✅ Exchange started\n');

    console.log(`📊 Subscribing to orderbook for ${TEST_SYMBOL}...\n`);

    const _unsubscribe = exchange.listenOrderBook(TEST_SYMBOL, (orderbook) => {
      updateCount++;
      lastUpdateTime = Date.now();

      if (orderbook.bids.length > 0) {
        lastBid = orderbook.bids[0].price;
      }
      if (orderbook.asks.length > 0) {
        lastAsk = orderbook.asks[0].price;
      }

      if (updateCount % 10 === 0) {
        console.log(`\n📊 Update #${updateCount}:`);
        console.log(`   Bid: ${lastBid} (${orderbook.bids.length} levels)`);
        console.log(`   Ask: ${lastAsk} (${orderbook.asks.length} levels)`);
        console.log(`   Spread: ${(lastAsk - lastBid).toFixed(2)}`);
      }
    });

    console.log('✅ Listening for orderbook updates...\n');
    console.log('Press Ctrl+C to stop\n');

    // Test reconnection after 30 seconds
    setTimeout(() => {
      console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
      console.log('🔄 Testing reconnection...');
      console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

      // Force close WebSocket to trigger reconnection
      if (wsPublic.ws) {
        console.log('Closing public WebSocket to trigger reconnection...');
        wsPublic.ws.close();
      }
    }, 30000);

    // Keep running until interrupted
    await new Promise(() => {});

  } catch (error: any) {
    console.error('\n❌ Error:', error.message);
    if (error.stack) {
      console.error('\nStack trace:');
      console.error(error.stack);
    }
    process.exit(1);
  } finally {
    exchange.dispose();
  }
}

main().catch((error) => {
  console.error('Unhandled error:', error);
  process.exit(1);
});
