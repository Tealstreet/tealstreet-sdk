/**
 * Test script for Arkham order placement and cancellation
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-order-placement.ts
 *
 * IMPORTANT: This test uses REAL orders with REAL money. Use minimal amounts!
 */

import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

// Load API credentials
const credentialsPath = path.join(__dirname, '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;
const BASE_URL = 'https://arkm.com/api';

console.log('⚠️  WARNING: This test places REAL orders with REAL money!\n');
console.log('📊 Testing Arkham Order Placement\n');

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
  const hexSecret = decodedSecret.toString('hex');
  const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
    .update(message)
    .digest('base64');

  return { signature, expiry };
}

/**
 * Test 1: Get market info for BTC_USDT to determine minimum order size
 */
async function getMarketInfo() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 1: Get Market Info for BTC_USDT');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/pairs`);
    const pairs = response.data;
    const btcPair = pairs.find((p: any) => p.symbol === 'BTC_USDT');

    if (btcPair) {
      console.log('✅ BTC_USDT market info retrieved:');
      console.log(`Symbol: ${btcPair.symbol}`);
      console.log(`Min Size: ${btcPair.min_size || btcPair.minSize || 'unknown'}`);
      console.log(`Min Notional: ${btcPair.min_notional || btcPair.minNotional || 'unknown'}`);
      console.log(`Step Size: ${btcPair.step_size || btcPair.stepSize || 'unknown'}`);
      console.log(`Tick Size: ${btcPair.tick_size || btcPair.tickSize || 'unknown'}`);
      console.log();

      return { success: true, data: btcPair };
    } else {
      console.log('❌ BTC_USDT pair not found\n');
      return { success: false };
    }
  } catch (error: any) {
    console.log('❌ Failed to get market info!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 2: Get current ticker to determine a safe limit price
 */
async function getCurrentPrice() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 2: Get Current BTC_USDT Price');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/ticker`, {
      params: { symbol: 'BTC_USDT' },
    });

    const ticker = response.data;
    console.log('✅ Current price retrieved:');
    console.log(`Current Price: $${ticker.price}`);
    console.log(`24h High: $${ticker.high24h}`);
    console.log(`24h Low: $${ticker.low24h}`);
    console.log();

    return {
      success: true,
      data: {
        lastPrice: parseFloat(ticker.price || '0'),
        bidPrice: parseFloat(ticker.price || '0'), // Use current price as estimate
        askPrice: parseFloat(ticker.price || '0'), // Use current price as estimate
      },
    };
  } catch (error: any) {
    console.log('❌ Failed to get current price!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 3: Place a small limit buy order (far from market price to avoid fill)
 */
async function placeLimitOrder(
  symbol: string,
  side: 'buy' | 'sell',
  size: string,
  price: string
) {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log(`Test 3: Place ${side.toUpperCase()} Limit Order`);
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/orders/new';
  const method = 'POST';

  const orderData = {
    symbol,
    side,
    type: 'limitGtc', // Arkham uses limitGtc, not just 'limit'
    size,
    price,
  };

  const body = JSON.stringify(orderData);
  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint, body);

  console.log('Order Details:');
  console.log(`  Symbol: ${symbol}`);
  console.log(`  Side: ${side}`);
  console.log(`  Type: limit`);
  console.log(`  Size: ${size}`);
  console.log(`  Price: ${price}`);
  console.log(`  Time in Force: GTC\n`);

  try {
    const response = await axios.post(`${BASE_URL}${endpoint}`, orderData, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Order placed successfully!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return { success: true, data: response.data };
  } catch (error: any) {
    console.log('❌ Order placement failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false, error: error.response?.data };
  }
}

/**
 * Test 4: Get open orders
 */
async function getOpenOrders() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 4: Get Open Orders');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/orders';
  const method = 'GET';

  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    const orders = Array.isArray(response.data) ? response.data : response.data.orders || [];

    console.log('✅ Open orders retrieved:');
    console.log(`Total orders: ${orders.length}\n`);

    if (orders.length > 0) {
      orders.forEach((order: any, i: number) => {
        console.log(`Order ${i + 1}:`);
        console.log(`  ID: ${order.id || order.orderId}`);
        console.log(`  Symbol: ${order.symbol}`);
        console.log(`  Side: ${order.side}`);
        console.log(`  Type: ${order.type || order.orderType}`);
        console.log(`  Size: ${order.size || order.quantity}`);
        console.log(`  Price: ${order.price}`);
        console.log(`  Status: ${order.status}`);
        console.log();
      });
    }

    return { success: true, data: orders };
  } catch (error: any) {
    console.log('❌ Failed to get open orders!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 5: Cancel a specific order
 */
async function cancelOrder(orderId: string) {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 5: Cancel Order');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/orders/cancel';
  const method = 'POST';

  const cancelData = {
    orderId: orderId,
  };

  const body = JSON.stringify(cancelData);
  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint, body);

  console.log(`Cancelling order ID: ${orderId}\n`);

  try {
    const response = await axios.post(`${BASE_URL}${endpoint}`, cancelData, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Order cancelled successfully!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return { success: true, data: response.data };
  } catch (error: any) {
    console.log('❌ Order cancellation failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Run all tests
 */
async function runAllTests() {
  console.log('🚀 Starting Arkham Order Placement Tests\n');
  console.log('⚠️  CAUTION: Using minimal order sizes to minimize risk\n');

  // Step 1: Get market info
  const marketInfo = await getMarketInfo();
  if (!marketInfo.success) {
    console.log('❌ Cannot proceed without market info\n');
    return;
  }

  // Step 2: Get current price
  const priceInfo = await getCurrentPrice();
  if (!priceInfo.success || !priceInfo.data) {
    console.log('❌ Cannot proceed without current price\n');
    return;
  }

  // Step 3: Calculate a safe order (buy far below market to avoid fill)
  const currentBid = priceInfo.data.bidPrice;
  const safePrice = (currentBid * 0.5).toFixed(2); // 50% below market - won't fill
  const minSize = '0.0001'; // Minimal BTC size

  console.log(`📊 Calculated safe order parameters:`);
  console.log(`  Current bid: $${currentBid}`);
  console.log(`  Safe buy price: $${safePrice} (50% below market)`);
  console.log(`  Order size: ${minSize} BTC`);
  console.log(`  Estimated value: $${(parseFloat(minSize) * parseFloat(safePrice)).toFixed(2)}\n`);

  // Step 4: Place limit order
  const orderResult = await placeLimitOrder('BTC_USDT', 'buy', minSize, safePrice);

  if (!orderResult.success) {
    console.log('⚠️  Order placement failed. Checking if it was due to limits or parameters.\n');
    return;
  }

  // Extract order ID from response
  const orderId =
    orderResult.data?.id ||
    orderResult.data?.orderId ||
    orderResult.data?.order_id ||
    orderResult.data?.clientOrderId;

  if (!orderId) {
    console.log('⚠️  Order placed but no ID returned. Cannot cancel.\n');
    return;
  }

  // Step 5: Verify order appears in open orders
  await getOpenOrders();

  // Step 6: Cancel the order
  await cancelOrder(orderId);

  // Step 7: Verify order is cancelled
  await getOpenOrders();

  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📊 Test Results Summary');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  console.log('✅ Order placement and cancellation test completed!\n');
}

// Run tests
runAllTests().catch((err) => {
  console.error('Test error:', err);
  process.exit(1);
});
