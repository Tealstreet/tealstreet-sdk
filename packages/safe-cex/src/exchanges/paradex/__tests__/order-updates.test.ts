/**
 * Test script for Paradex order updates via WebSocket
 *
 * This script places an order and monitors WebSocket order updates
 * Run with: npx tsx packages/safe-cex/src/exchanges/paradex/test-order-updates.ts
 */

import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderSide, OrderType } from '../../../types';
import * as fs from 'fs';
import * as path from 'path';

async function main() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📝 Paradex Order Updates WebSocket Test');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  // Load credentials
  const credsPath = path.join(__dirname, '.apikey.json');
  if (!fs.existsSync(credsPath)) {
    console.error('❌ .apikey.json not found');
    process.exit(1);
  }

  const testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
  console.log('✓ Loaded credentials from .apikey.json');
  console.log(`  Account: ${testCreds.paradexAddress}`);
  console.log(`  Subkey: ${testCreds.subkeyAddress}\n`);

  // Create store and exchange
  const store = new DefaultStore();
  const exchange = new ParadexExchange(
    'paradex',
    {
      key: testCreds.apiKey,
      secret: '',
      testnet: testCreds.testnet || false,
      paradexAddress: testCreds.paradexAddress,
      subkeyAddress: testCreds.subkeyAddress,
    },
    store,
  );

  // Track order updates
  let orderUpdateCount = 0;
  const orderUpdates: any[] = [];

  // Subscribe to store changes to see order updates
  const unsubscribe = store.subscribe((state) => {
    if (state.orders && state.orders.length > 0) {
      const latestOrder = state.orders[state.orders.length - 1];
      orderUpdateCount++;
      orderUpdates.push({
        count: orderUpdateCount,
        time: new Date().toISOString(),
        order: latestOrder,
      });
      console.log(`\n📥 Order Update #${orderUpdateCount}:`);
      console.log(`   ID: ${latestOrder.id}`);
      console.log(`   Status: ${latestOrder.status}`);
      console.log(`   Symbol: ${latestOrder.symbol}`);
      console.log(`   Side: ${latestOrder.side}`);
      console.log(`   Filled: ${latestOrder.filled}/${latestOrder.amount}`);
    }
  });

  try {
    console.log('🔄 Starting exchange...');
    await exchange.start();
    console.log('✅ Exchange started\n');

    // Wait for WebSocket to connect
    await new Promise((resolve) => setTimeout(resolve, 2000));

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📝 Placing test order...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    const orderIds = await exchange.placeOrder({
      symbol: 'ETH-USD-PERP',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.01,
      price: 100, // Far below market
    });

    console.log(`✅ Order placed: ${orderIds[0]}\n`);

    console.log('⏳ Waiting 10 seconds for order updates via WebSocket...\n');
    await new Promise((resolve) => setTimeout(resolve, 10000));

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 Order Updates Summary');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    console.log(`Total order updates received: ${orderUpdateCount}`);

    if (orderUpdateCount === 0) {
      console.log('\n⚠️  WARNING: No order updates received via WebSocket!');
      console.log('   This might indicate an issue with the WebSocket subscription.');
    } else {
      console.log('\n✅ Order updates working correctly!');
      console.log('\nUpdate details:');
      orderUpdates.forEach((update) => {
        console.log(`  [${update.time}] Update #${update.count}: ${update.order.id} - ${update.order.status}`);
      });
    }

    // Clean up
    console.log('\n🧹 Cleaning up - canceling test order...');
    await exchange.cancelOrder(orderIds[0]);
    console.log('✓ Order canceled\n');

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    if (orderUpdateCount > 0) {
      console.log('✅ TEST PASSED!');
    } else {
      console.log('❌ TEST FAILED: No order updates received');
    }
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');

  } catch (error: any) {
    console.error('\n❌ Error:', error.message);
    if (error.stack) {
      console.error('\nStack trace:');
      console.error(error.stack);
    }
    process.exit(1);
  } finally {
    unsubscribe();
    exchange.dispose();
  }
}

main().catch((error) => {
  console.error('Unhandled error:', error);
  process.exit(1);
});
