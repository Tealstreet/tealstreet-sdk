import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { GrvtExchange } from '../grvt/grvt.exchange';
import { DydxExchange } from '../dydx/dydx.exchange';
import { BackpackExchange } from '../backpack/backpack.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';

/**
 * Integration test for order flow across GRVT, dYdX, and Backpack
 * Tests: placeOrder -> fetchOrders -> cancelOrder
 */

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet: boolean;
}

const TEST_ORDER_USD_VALUE = 10; // $10 test orders

describe.skip('Order Flow Integration - All Exchanges', () => {
  let grvtExchange: GrvtExchange;
  let dydxExchange: DydxExchange;
  let backpackExchange: BackpackExchange;

  let grvtConfig: ApiKeyConfig;
  let dydxConfig: ApiKeyConfig;
  let backpackConfig: ApiKeyConfig;

  const placedOrderIds: { exchange: string; orderId: string }[] = [];

  beforeAll(async () => {
    // Load API credentials
    try {
      grvtConfig = JSON.parse(readFileSync(join(__dirname, '../grvt/.apikey.json'), 'utf-8'));
      dydxConfig = JSON.parse(readFileSync(join(__dirname, '../dydx/.apikey.json'), 'utf-8'));
      backpackConfig = JSON.parse(readFileSync(join(__dirname, '../backpack/.apikey.json'), 'utf-8'));
    } catch (err) {
      console.error('Failed to load API credentials:', err);
      throw err;
    }

    console.log('\n===========================================');
    console.log('Order Flow Integration Test');
    console.log('===========================================');
    console.log('Testing: placeOrder -> fetchOrders -> cancelOrder');
    console.log('Order size: ~$' + TEST_ORDER_USD_VALUE);
    console.log('===========================================\n');
  }, 60000);

  afterAll(async () => {
    // Clean up any remaining orders
    console.log('\n===========================================');
    console.log('Cleanup: Cancelling any remaining test orders');
    console.log('===========================================\n');

    for (const { exchange: exchangeName, orderId } of placedOrderIds) {
      try {
        console.log(`Cleaning up ${exchangeName} order ${orderId}...`);
        // We'll need to implement cleanup logic here
      } catch (err) {
        console.warn(`Failed to cleanup ${exchangeName} order ${orderId}:`, err);
      }
    }
  });

  describe('GRVT Exchange', () => {
    beforeAll(async () => {
      const store = new DefaultStore();
      grvtExchange = new GrvtExchange('grvt', {
        key: grvtConfig.apiKey,
        secret: grvtConfig.secret,
        testnet: grvtConfig.testnet,
      }, store);

      console.log('\n--- GRVT Exchange Setup ---');
      await grvtExchange.validateAccount();

      // Fetch markets to get pricing
      const markets = await grvtExchange.fetchMarkets();
      store.update({ markets });

      console.log(`Loaded ${markets.length} GRVT markets`);
    }, 120000);

    it('should place, fetch, and cancel a small limit order', async () => {
      console.log('\n🔵 GRVT: Starting order flow test...\n');

      // Find ETH market
      const ethMarket = grvtExchange.store.markets.find(m =>
        m.symbol.includes('ETH') && m.symbol.includes('USDT') && !m.isSpot
      );

      if (!ethMarket) {
        console.log('⚠️  No ETH perpetual market found on GRVT, skipping test');
        return;
      }

      console.log(`Using market: ${ethMarket.symbol}`);

      // Get current price estimate (we'll place order far from market)
      const estimatedPrice = 3000; // Rough ETH price
      const orderAmount = TEST_ORDER_USD_VALUE / estimatedPrice;

      // Place limit order 20% below market (won't fill)
      const limitPrice = estimatedPrice * 0.8;

      console.log(`Placing limit BUY order:`);
      console.log(`  Amount: ${orderAmount.toFixed(6)} ETH`);
      console.log(`  Price: $${limitPrice.toFixed(2)}`);
      console.log(`  Notional: ~$${TEST_ORDER_USD_VALUE}`);

      const orderIds = await grvtExchange.placeOrder({
        symbol: ethMarket.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderAmount,
        price: limitPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      const orderId = orderIds[0];
      console.log(`✅ Order placed: ${orderId}\n`);

      placedOrderIds.push({ exchange: 'grvt', orderId });

      // Wait a moment for order to be registered
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Fetch orders
      console.log('Fetching orders...');
      const orders = await grvtExchange.fetchOrders([ethMarket.symbol]);

      console.log(`Found ${orders.length} order(s)`);

      // Verify our order appears
      const ourOrder = orders.find(o => o.id === orderId);
      expect(ourOrder).toBeDefined();

      if (ourOrder) {
        console.log(`✅ Order found in fetchOrders:`);
        console.log(`  ID: ${ourOrder.id}`);
        console.log(`  Symbol: ${ourOrder.symbol}`);
        console.log(`  Side: ${ourOrder.side}`);
        console.log(`  Type: ${ourOrder.type}`);
        console.log(`  Price: $${ourOrder.price}`);
        console.log(`  Amount: ${ourOrder.amount}`);
        console.log(`  Status: ${ourOrder.status}\n`);

        // Cancel the order
        console.log('Cancelling order...');
        await grvtExchange.cancelOrder(ourOrder);
        console.log(`✅ Order cancelled\n`);

        // Verify it's gone
        await new Promise(resolve => setTimeout(resolve, 2000));
        const ordersAfterCancel = await grvtExchange.fetchOrders([ethMarket.symbol]);
        const cancelledOrder = ordersAfterCancel.find(o => o.id === orderId);

        expect(cancelledOrder).toBeUndefined();
        console.log(`✅ Verified order no longer appears in fetchOrders\n`);
      }
    }, 120000);
  });

  describe('dYdX Exchange', () => {
    beforeAll(async () => {
      const store = new DefaultStore();
      dydxExchange = new DydxExchange('dydx', {
        key: dydxConfig.apiKey,
        secret: dydxConfig.secret,
        testnet: dydxConfig.testnet,
      }, store);

      console.log('\n--- dYdX Exchange Setup ---');
      await dydxExchange.validateAccount();

      const markets = await dydxExchange.fetchMarkets();
      store.update({ markets });

      console.log(`Loaded ${markets.length} dYdX markets`);
    }, 120000);

    it('should place, fetch, and cancel a small limit order', async () => {
      console.log('\n🟢 dYdX: Starting order flow test...\n');

      // Find ETH market
      const ethMarket = dydxExchange.store.markets.find(m =>
        m.symbol.includes('ETH') && m.symbol.includes('USD')
      );

      if (!ethMarket) {
        console.log('⚠️  No ETH market found on dYdX, skipping test');
        return;
      }

      console.log(`Using market: ${ethMarket.symbol}`);

      const estimatedPrice = 3000;
      const orderAmount = TEST_ORDER_USD_VALUE / estimatedPrice;
      const limitPrice = estimatedPrice * 0.8;

      console.log(`Placing limit BUY order:`);
      console.log(`  Amount: ${orderAmount.toFixed(6)} ETH`);
      console.log(`  Price: $${limitPrice.toFixed(2)}`);
      console.log(`  Notional: ~$${TEST_ORDER_USD_VALUE}`);

      const orderIds = await dydxExchange.placeOrder({
        symbol: ethMarket.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderAmount,
        price: limitPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      const orderId = orderIds[0];
      console.log(`✅ Order placed: ${orderId}\n`);

      placedOrderIds.push({ exchange: 'dydx', orderId });

      await new Promise(resolve => setTimeout(resolve, 3000));

      console.log('Fetching orders...');
      const orders = await dydxExchange.fetchOrders([ethMarket.symbol]);

      console.log(`Found ${orders.length} order(s)`);

      const ourOrder = orders.find(o => o.id === orderId || (o.info as any)?.clientId === orderId);
      expect(ourOrder).toBeDefined();

      if (ourOrder) {
        console.log(`✅ Order found in fetchOrders:`);
        console.log(`  ID: ${ourOrder.id}`);
        console.log(`  Symbol: ${ourOrder.symbol}`);
        console.log(`  Side: ${ourOrder.side}`);
        console.log(`  Type: ${ourOrder.type}`);
        console.log(`  Price: $${ourOrder.price}`);
        console.log(`  Amount: ${ourOrder.amount}`);
        console.log(`  Status: ${ourOrder.status}\n`);

        console.log('Cancelling order...');
        await dydxExchange.cancelOrder(ourOrder);
        console.log(`✅ Order cancelled\n`);

        await new Promise(resolve => setTimeout(resolve, 3000));
        const ordersAfterCancel = await dydxExchange.fetchOrders([ethMarket.symbol]);
        const cancelledOrder = ordersAfterCancel.find(o => o.id === ourOrder.id);

        expect(cancelledOrder).toBeUndefined();
        console.log(`✅ Verified order no longer appears in fetchOrders\n`);
      }
    }, 120000);
  });

  describe('Backpack Exchange', () => {
    beforeAll(async () => {
      const store = new DefaultStore();
      backpackExchange = new BackpackExchange('backpack', {
        key: backpackConfig.apiKey,
        secret: backpackConfig.secret,
        testnet: backpackConfig.testnet,
        tealstreetProxy: true,
      }, store);

      console.log('\n--- Backpack Exchange Setup ---');
      await backpackExchange.validateAccount();

      const markets = await backpackExchange.fetchMarkets();
      store.update({ markets });

      console.log(`Loaded ${markets.length} Backpack markets`);
    }, 120000);

    it('should place, fetch, and cancel a small limit order', async () => {
      console.log('\n🟣 Backpack: Starting order flow test...\n');

      // Find SOL market (Backpack is Solana-based)
      const solMarket = backpackExchange.store.markets.find(m =>
        m.symbol.includes('SOL') && m.symbol.includes('USDC')
      );

      if (!solMarket) {
        console.log('⚠️  No SOL market found on Backpack, skipping test');
        return;
      }

      console.log(`Using market: ${solMarket.symbol}`);

      const estimatedPrice = 150; // Rough SOL price
      const orderAmount = TEST_ORDER_USD_VALUE / estimatedPrice;
      const limitPrice = estimatedPrice * 0.8;

      console.log(`Placing limit BUY order:`);
      console.log(`  Amount: ${orderAmount.toFixed(6)} SOL`);
      console.log(`  Price: $${limitPrice.toFixed(2)}`);
      console.log(`  Notional: ~$${TEST_ORDER_USD_VALUE}`);

      const orderIds = await backpackExchange.placeOrder({
        symbol: solMarket.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderAmount,
        price: limitPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      const orderId = orderIds[0];
      console.log(`✅ Order placed: ${orderId}\n`);

      placedOrderIds.push({ exchange: 'backpack', orderId });

      await new Promise(resolve => setTimeout(resolve, 2000));

      console.log('Fetching orders...');
      const orders = await backpackExchange.fetchOrders();

      console.log(`Found ${orders.length} order(s)`);

      const ourOrder = orders.find(o => o.id === orderId);
      expect(ourOrder).toBeDefined();

      if (ourOrder) {
        console.log(`✅ Order found in fetchOrders:`);
        console.log(`  ID: ${ourOrder.id}`);
        console.log(`  Symbol: ${ourOrder.symbol}`);
        console.log(`  Side: ${ourOrder.side}`);
        console.log(`  Type: ${ourOrder.type}`);
        console.log(`  Price: $${ourOrder.price}`);
        console.log(`  Amount: ${ourOrder.amount}`);
        console.log(`  Status: ${ourOrder.status}\n`);

        console.log('Cancelling order...');
        await backpackExchange.cancelOrder(ourOrder);
        console.log(`✅ Order cancelled\n`);

        await new Promise(resolve => setTimeout(resolve, 2000));
        const ordersAfterCancel = await backpackExchange.fetchOrders();
        const cancelledOrder = ordersAfterCancel.find(o => o.id === orderId);

        expect(cancelledOrder).toBeUndefined();
        console.log(`✅ Verified order no longer appears in fetchOrders\n`);
      }
    }, 120000);
  });
});

/**
 * To run this test:
 *
 * 1. Ensure API credentials are set up in .apikey.json files for each exchange
 * 2. Remove .skip from the describe block above
 * 3. Run: yarn test order-flow-integration.test.ts
 *
 * The test will:
 * - Place a ~$10 limit order on each exchange (far from market to avoid fills)
 * - Fetch orders and verify the placed order appears
 * - Cancel the order
 * - Verify the order no longer appears in fetchOrders
 */
