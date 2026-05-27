/**
 * Test script for Arkham trigger order placement, fetching, and cancellation
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/__tests__/trigger-orders.test.ts
 *
 * IMPORTANT: This test uses REAL trigger orders. Use minimal amounts!
 */

import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

// Load API credentials
const credentialsPath = path.join(__dirname, '../.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;
const BASE_URL = 'https://arkm.com/api';

console.log('⚠️  WARNING: This test places REAL trigger orders!\n');
console.log('📊 Testing Arkham Trigger Orders\n');

function createSignature(
  apiKey: string,
  apiSecret: string,
  method: string,
  path: string,
  body: string = ''
): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000;
  const expiry = timestamp + 5 * 1000000; // 5 seconds

  const message = `${apiKey}${expiry}${method}${path}${body}`;

  const decodedSecret = Buffer.from(apiSecret, 'base64');
  const signature = createHmac('sha256', decodedSecret)
    .update(message)
    .digest('base64');

  return { signature, expiry };
}

/**
 * Test 1: Get current ticker to determine safe trigger price
 */
async function getTicker() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 1: Get Ticker for BTC_USDT_PERP');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/ticker`, {
      params: { symbol: 'BTC_USDT_PERP' },
    });

    const ticker = response.data;
    console.log('✅ Ticker retrieved successfully:');
    console.log(`   Last Price: ${ticker.price}`);
    console.log(`   Mark Price: ${ticker.markPrice}`);
    console.log('');

    return parseFloat(ticker.price);
  } catch (error: any) {
    console.error('❌ Error getting ticker:', error.response?.data || error.message);
    throw error;
  }
}

/**
 * Test 2: Place a stop loss trigger order
 */
async function placeStopLossTriggerOrder(currentPrice: number) {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 2: Place Stop Loss Trigger Order');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  // Set stop loss trigger at 5% below current price (won't trigger immediately)
  const triggerPrice = Math.floor(currentPrice * 0.95);

  const payload = {
    symbol: 'BTC_USDT_PERP',
    side: 'buy',
    type: 'market',
    size: '0.001',
    triggerPrice: triggerPrice.toString(),
    triggerType: 'stopLoss',
    triggerPriceType: 'last',
    reduceOnly: true, // Trigger orders are typically used to close positions
  };

  try {
    const body = JSON.stringify(payload);
    const { signature, expiry } = createSignature(API_KEY, API_SECRET, 'POST', '/trigger-orders/new', body);

    const response = await axios.post(`${BASE_URL}/trigger-orders/new`, payload, {
      headers: {
        'Content-Type': 'application/json',
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1001',
      },
    });

    console.log('✅ Stop Loss trigger order placed successfully:');
    console.log(`   Trigger Order ID: ${response.data.triggerOrderId}`);
    console.log(`   Symbol: ${response.data.symbol}`);
    console.log(`   Side: ${response.data.side}`);
    console.log(`   Size: ${response.data.size}`);
    console.log(`   Current Price: ${currentPrice}`);
    console.log(`   Trigger Price: ${triggerPrice} (5% below)`);
    console.log('');

    return response.data.triggerOrderId;
  } catch (error: any) {
    console.error('❌ Error placing trigger order:', error.response?.data || error.message);
    throw error;
  }
}

/**
 * Test 3: Fetch all trigger orders
 */
async function fetchTriggerOrders() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 3: Fetch All Trigger Orders');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const { signature, expiry } = createSignature(API_KEY, API_SECRET, 'GET', '/trigger-orders');

    const response = await axios.get(`${BASE_URL}/trigger-orders`, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1001',
      },
    });

    console.log('✅ Trigger orders fetched successfully:');
    console.log(`   Total trigger orders: ${response.data.length}`);

    response.data.forEach((order: any, index: number) => {
      console.log(`\n   Order ${index + 1}:`);
      console.log(`     Trigger Order ID: ${order.triggerOrderId}`);
      console.log(`     Symbol: ${order.symbol}`);
      console.log(`     Side: ${order.side}`);
      console.log(`     Type: ${order.type}`);
      console.log(`     Size: ${order.size}`);
      console.log(`     Trigger Price: ${order.triggerPrice}`);
      console.log(`     Trigger Type: ${order.triggerType}`);
      console.log(`     Trigger Price Type: ${order.triggerPriceType}`);
      console.log(`     Status: ${order.status}`);
      console.log(`     Reduce Only: ${order.reduceOnly}`);
    });
    console.log('');

    return response.data;
  } catch (error: any) {
    console.error('❌ Error fetching trigger orders:', error.response?.data || error.message);
    throw error;
  }
}

/**
 * Test 4: Cancel a trigger order
 */
async function cancelTriggerOrder(triggerOrderId: number) {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 4: Cancel Trigger Order');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const payload = {
    triggerOrderId,
  };

  try {
    const body = JSON.stringify(payload);
    const { signature, expiry } = createSignature(API_KEY, API_SECRET, 'POST', '/trigger-orders/cancel', body);

    await axios.post(`${BASE_URL}/trigger-orders/cancel`, payload, {
      headers: {
        'Content-Type': 'application/json',
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1001',
      },
    });

    console.log('✅ Trigger order cancelled successfully');
    console.log(`   Cancelled Trigger Order ID: ${triggerOrderId}`);
    console.log('');
  } catch (error: any) {
    console.error('❌ Error cancelling trigger order:', error.response?.data || error.message);
    throw error;
  }
}

/**
 * Main test flow
 */
async function runTests() {
  try {
    // Test 1: Get current price
    const currentPrice = await getTicker();

    // Test 2: Place stop loss trigger order
    const triggerOrderId = await placeStopLossTriggerOrder(currentPrice);

    // Wait a moment for the order to be processed
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Test 3: Fetch all trigger orders
    await fetchTriggerOrders();

    // Test 4: Cancel the trigger order
    await cancelTriggerOrder(triggerOrderId);

    // Verify cancellation
    await new Promise((resolve) => setTimeout(resolve, 1000));
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Verification: Fetch Trigger Orders After Cancel');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    await fetchTriggerOrders();

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('✅ All tests completed successfully!');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
  } catch (error) {
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('❌ Tests failed');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    process.exit(1);
  }
}

// Run the tests
runTests();
