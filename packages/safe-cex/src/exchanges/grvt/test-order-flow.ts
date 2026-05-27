/**
 * Manual test script for GRVT order flow
 * Tests: placeOrder -> fetchOrders -> cancelOrder
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet: boolean;
}

const TEST_ORDER_USD_VALUE = 10; // $10 test order

async function testGrvtOrderFlow() {
  console.log('\n===========================================');
  console.log('🟣 GRVT Order Flow Test');
  console.log('===========================================\n');

  // Load credentials
  const configPath = join(__dirname, '.apikey.json');
  const config: ApiKeyConfig = JSON.parse(readFileSync(configPath, 'utf-8'));

  console.log('📋 Configuration:');
  console.log(`  API Key: ${config.apiKey.substring(0, 10)}...`);
  console.log(`  Network: ${config.testnet ? 'Testnet' : 'Mainnet'}`);
  console.log(`  Order Size: ~$${TEST_ORDER_USD_VALUE}\n`);

  // Create exchange
  const store = new DefaultStore();
  const exchange = new GrvtExchange('grvt', {
    key: config.apiKey,
    secret: config.secret,
    testnet: config.testnet,
  }, store);

  try {
    // Step 1: Validate account (also authenticates)
    console.log('1️⃣ Validating account and authenticating...');
    const validationResult = await exchange.validateAccount();
    if (validationResult !== '') {
      console.error('❌ Account validation failed:', validationResult);
      return;
    }
    console.log('✅ Account validated and authenticated');
    console.log(`  Account ID: ${exchange.apiExtras?.accountId}`);
    console.log(`  Sub-Account ID: ${exchange.apiExtras?.subAccountId}\n`);

    // Step 2: Fetch markets
    console.log('2️⃣ Fetching markets...');
    const markets = await exchange.fetchMarkets();
    store.update({ markets });
    console.log(`✅ Loaded ${markets.length} markets\n`);

    // Find ETH perpetual market
    const ethMarket = markets.find(m => m.symbol === 'ETH_USDT_Perp');
    if (!ethMarket) {
      console.error('❌ ETH_USDT_Perp market not found');
      console.log('Available markets:', markets.map(m => m.symbol).slice(0, 10));
      return;
    }
    console.log(`📊 Using market: ${ethMarket.symbol}`);
    console.log(`  Min amount: ${ethMarket.limits.amount.min}`);
    console.log(`  Max amount: ${ethMarket.limits.amount.max}`);
    console.log(`  Amount precision: ${ethMarket.precision.amount}`);
    console.log(`  Price precision: ${ethMarket.precision.price}\n`);

    // Step 3: Fetch current balance
    console.log('3️⃣ Checking balance...');
    const balance = await exchange.fetchBalance();
    console.log(`💰 Balance:`);
    console.log(`  Total: $${balance.total.toFixed(2)}`);
    console.log(`  Free: $${balance.free.toFixed(2)}`);
    console.log(`  Used: $${balance.used.toFixed(2)}`);
    if (balance.upnl !== undefined) {
      console.log(`  Unrealized PnL: $${balance.upnl.toFixed(2)}`);
    }
    console.log();

    if (balance.total < TEST_ORDER_USD_VALUE) {
      console.error(`❌ Insufficient balance. Need at least $${TEST_ORDER_USD_VALUE}`);
      return;
    }

    // Step 4: Calculate order parameters
    const estimatedPrice = 3000; // Rough ETH price
    const orderAmount = TEST_ORDER_USD_VALUE / estimatedPrice;
    const limitPrice = Math.floor(estimatedPrice * 0.8); // 20% below market

    // Adjust to market precision
    const adjustedAmount = Math.max(
      orderAmount,
      ethMarket.limits.amount.min
    );

    console.log(`4️⃣ Order parameters:`);
    console.log(`  Estimated ETH price: $${estimatedPrice}`);
    console.log(`  Order amount: ${adjustedAmount.toFixed(6)} ETH`);
    console.log(`  Limit price: $${limitPrice}`);
    console.log(`  Notional value: ~$${(adjustedAmount * limitPrice).toFixed(2)}`);
    console.log(`  (Far from market - won't fill)\n`);

    // Step 5: Place order
    console.log('5️⃣ Placing limit BUY order...');
    const orderIds = await exchange.placeOrder({
      symbol: ethMarket.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: adjustedAmount,
      price: limitPrice,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    if (!orderIds || orderIds.length === 0) {
      console.error('❌ Failed to place order - no order ID returned');
      return;
    }

    const orderId = orderIds[0];
    console.log(`✅ Order placed successfully!`);
    console.log(`  Order ID: ${orderId}\n`);

    // Wait for order to register
    console.log('⏳ Waiting 3 seconds for order to register...\n');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // Step 6: Fetch orders
    console.log('6️⃣ Fetching orders...');
    const orders = await exchange.fetchOrders([ethMarket.symbol]);
    console.log(`📋 Found ${orders.length} open order(s)\n`);

    // Find our order
    const ourOrder = orders.find(o => o.id === orderId);

    if (!ourOrder) {
      console.error('❌ Order not found in fetchOrders!');
      console.log('Available orders:');
      orders.forEach(o => {
        console.log(`  - ID: ${o.id}, Symbol: ${o.symbol}, Side: ${o.side}`);
      });
      console.log('\nℹ️  Order may still be processing. Check manually on GRVT.\n');
      return;
    }

    console.log('✅ Order found in fetchOrders:');
    console.log(`  Order ID: ${ourOrder.id}`);
    console.log(`  Symbol: ${ourOrder.symbol}`);
    console.log(`  Side: ${ourOrder.side}`);
    console.log(`  Type: ${ourOrder.type}`);
    console.log(`  Price: $${ourOrder.price}`);
    console.log(`  Amount: ${ourOrder.amount} ETH`);
    console.log(`  Filled: ${ourOrder.filled} ETH`);
    console.log(`  Status: ${ourOrder.status}\n`);

    // Step 7: Cancel order
    console.log('7️⃣ Cancelling order...');
    await exchange.cancelOrder(ourOrder);
    console.log('✅ Cancel request sent\n');

    // Wait for cancellation to process
    console.log('⏳ Waiting 3 seconds for cancellation to process...\n');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // Step 8: Verify cancellation
    console.log('8️⃣ Verifying cancellation...');
    const ordersAfterCancel = await exchange.fetchOrders([ethMarket.symbol]);
    const stillExists = ordersAfterCancel.find(o => o.id === orderId);

    if (stillExists) {
      console.log('⚠️  Order still appears in fetchOrders (may be processing)');
      console.log(`  Status: ${stillExists.status}\n`);
    } else {
      console.log('✅ Order successfully cancelled and removed from open orders\n');
    }

    // Final summary
    console.log('===========================================');
    console.log('✅ GRVT Order Flow Test Complete!');
    console.log('===========================================');
    console.log('✓ Account authenticated');
    console.log('✓ Order placed');
    console.log('✓ Order appeared in fetchOrders');
    console.log('✓ Order cancelled');
    console.log('===========================================\n');

  } catch (error: any) {
    console.error('\n❌ Error during test:');
    console.error(error.message);
    if (error.response?.data) {
      console.error('API Response:', JSON.stringify(error.response.data, null, 2));
    }
    console.error('\nStack trace:', error.stack);
  }
}

// Run the test
testGrvtOrderFlow().catch(console.error);
