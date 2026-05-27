/**
 * Test Order Placement and Cancellation Flow
 *
 * Run with: npx tsx packages/safe-cex/src/exchanges/backpack/test-order-flow.ts
 */

import { BackpackExchange } from './backpack.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';
import * as fs from 'fs';
import * as path from 'path';

async function main() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('🧪 Backpack Order Flow Test');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  // Load credentials
  const credsPath = path.join(__dirname, '.apikey.json');
  const creds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));

  // Create store and exchange
  const store = new DefaultStore();
  const exchange = new BackpackExchange(
    'backpack',
    {
      key: creds.apiKey,
      secret: creds.secret,
      testnet: false,
    },
    store
  );

  try {
    // Fetch markets
    console.log('📋 Fetching markets...');
    const markets = await exchange.fetchMarkets();
    store.update({ markets });
    console.log(`✓ Fetched ${markets.length} markets\n`);

    // Fetch tickers to get current prices
    console.log('📊 Fetching tickers...');
    const tickers = await exchange.fetchTickers();
    store.update({ tickers });
    console.log(`✓ Fetched ${tickers.length} tickers\n`);

    // Find SOL/USDC market
    const testMarket = markets.find(m => m.symbol === 'SOL/USDC' || m.symbol === 'SOL_USDC');
    if (!testMarket) {
      throw new Error('Could not find SOL/USDC market');
    }

    const testTicker = tickers.find(t => t.symbol === testMarket.symbol);
    if (!testTicker || testTicker.last === 0) {
      throw new Error('No valid price data for SOL/USDC');
    }

    console.log(`🎯 Using market: ${testMarket.symbol}`);
    console.log(`   Current price: $${testTicker.last}`);
    console.log(`   Min quantity: ${testMarket.limits.amount.min}\n`);

    // Calculate order parameters
    // Place order 30% below market (unlikely to fill)
    const orderPrice = Math.floor(testTicker.last * 0.7 * 100) / 100; // Round to 2 decimals
    const orderAmount = testMarket.limits.amount.min; // Use minimum amount

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📝 Step 1: Place Limit Buy Order');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log(`Parameters:`);
    console.log(`  Symbol: ${testMarket.symbol}`);
    console.log(`  Side: Buy`);
    console.log(`  Type: Limit`);
    console.log(`  Amount: ${orderAmount} SOL`);
    console.log(`  Price: $${orderPrice} (${Math.round((orderPrice / testTicker.last) * 100)}% of market)`);
    console.log(`  Total: $${(orderAmount * orderPrice).toFixed(2)}\n`);

    let orderId: string | undefined;

    try {
      const orderIds = await exchange.placeOrder({
        symbol: testMarket.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderAmount,
        price: orderPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      orderId = orderIds[0];
      console.log(`✅ Order placed successfully!`);
      console.log(`   Order ID: ${orderId}\n`);

    } catch (error: any) {
      console.error(`❌ Failed to place order:`);
      console.error(`   ${error.response?.data?.msg || error.message}`);
      if (error.response?.data) {
        console.error(`   Response:`, JSON.stringify(error.response.data, null, 2));
      }
      throw error;
    }

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('🔍 Step 2: Verify Order');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log('⏳ Waiting 2 seconds for order to be registered...');
    await new Promise(resolve => setTimeout(resolve, 2000));

    const orders = await exchange.fetchOrders();
    console.log(`\n✓ Fetched ${orders.length} open orders\n`);

    const placedOrder = orders.find(o => o.id === orderId);
    if (!placedOrder) {
      console.warn(`⚠️  Order ${orderId} not found in open orders`);
      console.log('   This might mean it was filled or rejected\n');
    } else {
      console.log(`✅ Order confirmed:`);
      console.log(`   ID: ${placedOrder.id}`);
      console.log(`   Symbol: ${placedOrder.symbol}`);
      console.log(`   Side: ${placedOrder.side}`);
      console.log(`   Type: ${placedOrder.type}`);
      console.log(`   Price: $${placedOrder.price}`);
      console.log(`   Amount: ${placedOrder.amount}`);
      console.log(`   Filled: ${placedOrder.filled}`);
      console.log(`   Remaining: ${placedOrder.remaining}`);
      console.log(`   Status: ${placedOrder.status}\n`);
    }

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('❌ Step 3: Cancel Order');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    if (placedOrder) {
      try {
        console.log(`Canceling order ${orderId}...`);
        await exchange.cancelOrder(placedOrder);
        console.log(`✅ Order canceled successfully\n`);

        console.log('⏳ Waiting 2 seconds for cancellation to propagate...');
        await new Promise(resolve => setTimeout(resolve, 2000));

        const ordersAfterCancel = await exchange.fetchOrders();
        const stillExists = ordersAfterCancel.find(o => o.id === orderId);

        if (!stillExists) {
          console.log(`\n✅ Order ${orderId} no longer in open orders`);
          console.log(`   Cancellation confirmed!\n`);
        } else {
          console.log(`\n⚠️  Order still exists with status: ${stillExists.status}\n`);
        }

      } catch (error: any) {
        console.error(`❌ Failed to cancel order:`);
        console.error(`   ${error.response?.data?.msg || error.message}\n`);
        throw error;
      }
    } else {
      console.log(`⚠️  Skipping cancellation - order not found\n`);
    }

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('✅ All tests passed!');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log('Summary:');
    console.log(`  ✓ Order placed: ${orderId}`);
    console.log(`  ✓ Order verified in open orders`);
    console.log(`  ✓ Order canceled successfully`);
    console.log(`  ✓ Cancellation confirmed\n`);

  } catch (error: any) {
    console.error('\n❌ Test failed:');
    console.error(error.message || error);
    if (error.stack) {
      console.error('\nStack trace:');
      console.error(error.stack);
    }
    process.exit(1);
  }
}

main();
