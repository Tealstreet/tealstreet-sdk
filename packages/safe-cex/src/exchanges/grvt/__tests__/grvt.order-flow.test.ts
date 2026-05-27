/**
 * @vitest-environment node
 */
import type { ExchangeOptions } from '../../../types';

import { existsSync, readFileSync } from 'fs';
import { resolve } from 'path';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { DefaultStore } from '../../../store/store.base';
import { OrderSide, OrderType } from '../../../types';
import { GrvtExchange } from '../grvt.exchange';

/**
 * Integration test for GRVT order flow:
 * 1. Place a $100 BTC futures long order
 * 2. Fetch orders to verify it was placed
 * 3. Cancel the order
 */
describe('GRVT Order Flow Integration Test', () => {
  let exchange: GrvtExchange | null = null;
  let store: DefaultStore;
  let credentials: {
    apiKey?: string;
    secret?: string;
    testnet?: boolean;
    accountId?: string;
    subAccountId?: string;
    grvtTradingAccountId?: string;
    grvtSignerAddress?: string;
  } = {};
  let placedOrderId: string | null = null;

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
      subaccount: credentials.subAccountId, // Pass subAccountId via subaccount field
      grvtTradingAccountId: credentials.grvtTradingAccountId, // Trading account ID for GRVT
      grvtSignerAddress: credentials.grvtSignerAddress, // Signer address for order signing
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

  it('should place a ~$100 BTC long position order (0.001 BTC)', async () => {
    if (!exchange || !credentials.apiKey || !credentials.secret) {
      console.log('⚠️ Skipping - exchange not initialized or no credentials');
      return;
    }

    console.log('\n📊 Placing 0.001 BTC long order (~$100)...');

    // Get BTC_USDT_Perp market
    const market = store.markets.find((m) => m.id === 'BTC_USDT_Perp');
    expect(market).toBeDefined();

    if (!market) {
      throw new Error('BTC_USDT_Perp market not found');
    }

    console.log(`   Market: ${market.id}`);
    console.log(`   Price precision: ${market.precision.price}`);
    console.log(`   Amount precision: ${market.precision.amount}`);
    console.log(`   Min amount: ${market.limits.amount.min}`);
    console.log(`   Max amount: ${market.limits.amount.max}`);

    // Use a reasonable fixed price for BTC (around $95,000)
    // This ensures the test is deterministic and doesn't depend on live market data
    const currentPrice = 95000;
    console.log(`   Using fixed BTC price: $${currentPrice.toLocaleString()}`);

    // Use 0.001 BTC for ~$100 notional value
    const orderSize = 0.001;
    const notionalValue = orderSize * currentPrice;

    console.log(`   Notional: $${notionalValue}`);
    console.log(`   Order size: ${orderSize.toFixed(6)} BTC`);

    // Place order at current price - 5% (to ensure it's not immediately filled)
    const orderPrice = currentPrice * 0.95;
    console.log(`   Order price: $${orderPrice.toLocaleString()} (5% below market)`);

    // Place limit order
    const orderIds = await exchange.placeOrder({
      symbol: 'BTC_USDT_Perp',
      type: OrderType.Limit,
      side: OrderSide.Buy, // Long position
      amount: orderSize,
      price: orderPrice,
    });

    expect(orderIds).toBeDefined();
    expect(Array.isArray(orderIds)).toBe(true);
    expect(orderIds.length).toBeGreaterThan(0);

    placedOrderId = orderIds[0];

    console.log(`\n✅ Order placed successfully!`);
    console.log(`   Order ID: ${placedOrderId}`);
  }, 30000);

  it('should fetch orders and find the placed order', async () => {
    if (!exchange || !placedOrderId) {
      console.log('⚠️ Skipping - no order was placed');
      return;
    }

    console.log(`\n🔍 Fetching orders...`);

    // Wait a moment for order to propagate
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Fetch open orders for BTC_USDT_Perp
    const orders = await exchange.fetchOpenOrders('BTC_USDT_Perp');

    expect(orders).toBeDefined();
    expect(Array.isArray(orders)).toBe(true);

    console.log(`   Found ${orders.length} open order(s)`);

    // Find our placed order
    const ourOrder = orders.find((o) => o.id === placedOrderId);

    expect(ourOrder).toBeDefined();
    expect(ourOrder?.id).toBe(placedOrderId);
    expect(ourOrder?.symbol).toBe('BTC_USDT_Perp');
    expect(ourOrder?.side).toBe('BUY');

    console.log(`\n✅ Order found in fetchOpenOrders!`);
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
    const ordersBeforeCancel = await exchange.fetchOpenOrders('BTC_USDT_Perp');
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
    const orders = await exchange.fetchOpenOrders('BTC_USDT_Perp');
    const cancelledOrder = orders.find((o) => o.id === placedOrderId);

    expect(cancelledOrder).toBeUndefined();

    console.log(`✅ Order successfully cancelled and removed from open orders`);
  }, 30000);
});
