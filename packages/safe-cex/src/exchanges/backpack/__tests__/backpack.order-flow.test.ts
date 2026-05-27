/**
 * @vitest-environment node
 */

import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync, writeFileSync } from 'fs';
import { BackpackExchange } from '../backpack.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderType, OrderSide, type ExchangeOptions } from '../../../types';

// Restore real fetch for Backpack API (it was mocked globally in setupVitest.ts)
vi.unstubAllGlobals();
delete (global as any).fetch;

const logFile = resolve(__dirname, '../debug.log');
function log(message: string) {
  const timestamp = new Date().toISOString();
  const line = `[${timestamp}] ${message}\n`;
  try {
    writeFileSync(logFile, line, { flag: 'a' });
  } catch (e) {
    // Ignore
  }
  console.log(message);
}

/**
 * Integration test for Backpack order flow:
 * 1. Place a ~$100 BTC order
 * 2. Fetch orders to verify it was placed
 * 3. Cancel the order
 */
describe('Backpack Order Flow Integration Test', () => {
  let exchange: BackpackExchange | null = null;
  let store: DefaultStore;
  let credentials: {
    apiKey?: string;
    secret?: string;
    testnet?: boolean;
  } = {};
  let placedOrderId: string | null = null;

  beforeAll(async () => {
    // Try to load credentials from .apikey.json
    const credPath = resolve(__dirname, '../.apikey.json');
    if (existsSync(credPath)) {
      try {
        credentials = JSON.parse(readFileSync(credPath, 'utf-8'));
        log('✅ Loaded API credentials from .apikey.json');
      } catch (error) {
        log('⚠️ Error loading credentials: ' + error);
      }
    } else {
      log('⚠️ No .apikey.json file found');
      log('💡 Create a .apikey.json file with your Backpack credentials to run this test');
      return;
    }

    // Create store
    store = new DefaultStore();

    // Initialize exchange
    const options: ExchangeOptions = {
      key: credentials.apiKey || '',
      secret: credentials.secret || '',
      testnet: credentials.testnet !== false, // Default to testnet for safety
    };

    exchange = new BackpackExchange('backpack', options, store);
    log('\n🔌 Initializing Backpack exchange...');

    // Listen for errors (but don't fail on WebSocket connection errors during startup)
    exchange.on('error', (error: string) => {
      log('   Exchange error event: ' + error);
    });

    try {
      // Start the exchange (this should load all initial data)
      // Note: This may throw WebSocket-related errors during startup
      await exchange.start();
      log('✅ Exchange started (start() completed)');
    } catch (error: any) {
      // WebSocket connection errors during startup are expected
      log('⚠️ Exchange start() threw error (may be WebSocket related):');
      log('   Error: ' + error.message);
    }

    // Wait for WebSocket connections to establish
    log('   Waiting for connections...');
    await new Promise((resolve) => setTimeout(resolve, 5000));

    // Check if markets were loaded
    if (store.markets.length > 0) {
      log('✅ Exchange initialized successfully');
      log(`   Markets loaded: ${store.markets.length}`);
    } else {
      log('❌ No markets loaded - exchange may not be working');
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

  it('should place a ~$100 BTC order', async () => {
    if (!exchange || !credentials.apiKey || !credentials.secret) {
      log('⚠️ Skipping - exchange not initialized or no credentials');
      return;
    }

    log('\n📊 Placing ~$100 BTC order...');

    // Get BTC_USDC market
    const market = store.markets.find((m) => m.symbol === 'BTC_USDC' || m.id === 'BTC_USDC');
    expect(market).toBeDefined();

    if (!market) {
      throw new Error('BTC_USDC market not found');
    }

    log(`   Market: ${market.id}`);
    log(`   Price precision: ${market.precision.price}`);
    log(`   Amount precision: ${market.precision.amount}`);
    log(`   Min amount: ${market.limits.amount.min}`);
    log(`   Max amount: ${market.limits.amount.max}`);

    // Use a reasonable fixed price for BTC (around $95,000)
    // This ensures the test is deterministic and doesn't depend on live market data
    const currentPrice = 95000;
    log(`   Using fixed BTC price: $${currentPrice.toLocaleString()}`);

    // Calculate order size for ~$100 notional value
    const rawOrderSize = 100 / currentPrice;

    // Round to market precision (amount precision: 0.00001 = 5 decimal places)
    const precisionDecimals = String(market.precision.amount).split('.')[1]?.length || 0;
    const orderSize = Number(rawOrderSize.toFixed(precisionDecimals));
    const notionalValue = orderSize * currentPrice;

    log(`   Notional: $${notionalValue.toFixed(2)}`);
    log(`   Order size: ${orderSize} BTC (rounded to ${precisionDecimals} decimals)`);

    // Place order at current price - 5% (to ensure it's not immediately filled)
    const rawOrderPrice = currentPrice * 0.95;
    // Round price to market precision (price precision: 0.1 = 1 decimal place)
    const pricePrecisionDecimals = String(market.precision.price).split('.')[1]?.length || 0;
    const orderPrice = Number(rawOrderPrice.toFixed(pricePrecisionDecimals));
    log(`   Order price: $${orderPrice.toLocaleString()} (5% below market)`);

    try {
      // Place limit order
      const orderIds = await exchange.placeOrder({
        symbol: market.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderSize,
        price: orderPrice,
      });

      expect(orderIds).toBeDefined();
      expect(Array.isArray(orderIds)).toBe(true);
      expect(orderIds.length).toBeGreaterThan(0);

      placedOrderId = orderIds[0];

      log(`\n✅ Order placed successfully!`);
      log(`   Order ID: ${placedOrderId}`);
    } catch (error: any) {
      log('❌ Order placement failed!');
      log('   Error: ' + error.message);
      if (error.response) {
        log('   Status: ' + error.response.status);
        log('   Data: ' + JSON.stringify(error.response.data, null, 2));
      }
      if (error.stack) {
        log('   Stack: ' + error.stack);
      }
      throw error;
    }
  }, 30000);

  it('should fetch orders and find the placed order', async () => {
    if (!exchange || !placedOrderId) {
      console.log('⚠️ Skipping - no order was placed');
      return;
    }

    console.log(`\n🔍 Fetching orders...`);

    // Wait a moment for order to propagate
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Fetch open orders
    const orders = await exchange.fetchOrders();

    expect(orders).toBeDefined();
    expect(Array.isArray(orders)).toBe(true);

    console.log(`   Found ${orders.length} open order(s)`);

    // Find our placed order
    const ourOrder = orders.find((o) => o.id === placedOrderId);

    expect(ourOrder).toBeDefined();
    expect(ourOrder?.id).toBe(placedOrderId);
    expect(ourOrder?.symbol).toMatch(/BTC[_-]USDC/);
    expect(ourOrder?.side).toBe('BUY');

    console.log(`\n✅ Order found in fetchOrders!`);
    console.log(`   Order ID: ${ourOrder?.id}`);
    console.log(`   Symbol: ${ourOrder?.symbol}`);
    console.log(`   Side: ${ourOrder?.side}`);
    console.log(`   Amount: ${ourOrder?.amount} BTC`);
    console.log(`   Price: $${ourOrder?.price?.toLocaleString()}`);
    console.log(`   Status: ${ourOrder?.status}`);
  }, 30000);

  it('should cancel the placed order', async () => {
    if (!exchange || !placedOrderId) {
      console.log('⚠️ Skipping - no order to cancel');
      return;
    }

    console.log(`\n🗑️ Cancelling order ${placedOrderId}...`);

    // Fetch the order first
    const ordersBeforeCancel = await exchange.fetchOrders();
    const orderToCancel = ordersBeforeCancel.find((o) => o.id === placedOrderId);

    if (!orderToCancel) {
      throw new Error('Order not found for cancellation');
    }

    // Cancel the order
    await exchange.cancelOrder(orderToCancel);

    console.log(`✅ Cancel request sent`);

    // Wait a moment for cancellation to propagate
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Verify order is no longer in open orders
    const orders = await exchange.fetchOrders();
    const cancelledOrder = orders.find((o) => o.id === placedOrderId);

    expect(cancelledOrder).toBeUndefined();

    console.log(`✅ Order successfully cancelled and removed from open orders`);
  }, 30000);
});
