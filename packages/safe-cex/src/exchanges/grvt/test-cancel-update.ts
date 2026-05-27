#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testCancelAndUpdate() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Cancel & Update Orders Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Trading Account ID:', config.grvtTradingAccountId);
  console.log('========================================\n');

  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: config.apiKey,
      secret: config.secret,
      testnet: config.testnet || false,
      subaccount: config.subaccountId,
      grvtSignerAddress: config.grvtSignerAddress,
      grvtTradingAccountId: config.grvtTradingAccountId,
    },
    store
  );

  try {
    console.log('1. Authenticating...');
    await exchange.authenticate();
    console.log('✅ Authenticated\n');

    // Fetch markets
    const markets = await exchange.fetchMarkets();
    store.update({ markets });

    console.log('2. Fetching current orders...');
    let orders = await exchange.fetchOpenOrders();
    console.log(`✅ Found ${orders.length} orders\n`);

    // Test 1: Place a test order
    console.log('3. Placing a test BTC limit order @ $80,000...');
    const newOrderIds = await exchange.placeOrder({
      symbol: 'BTC_USDT_Perp',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.001,
      price: 80000,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });
    console.log(`✅ Order placed: ${newOrderIds[0]}\n`);

    // Wait a moment for order to be registered
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Fetch orders again
    orders = await exchange.fetchOpenOrders();
    const testOrder = orders.find(o => o.id === newOrderIds[0]);
    
    if (!testOrder) {
      throw new Error('Test order not found');
    }

    console.log(`Order details: ${testOrder.symbol} ${testOrder.side} ${testOrder.amount} @ $${testOrder.price}\n`);

    // Test 2: Update order price
    console.log('4. Updating order price from $80,000 to $78,000...');
    const updatedOrderIds = await exchange.updateOrder({
      order: testOrder,
      update: { price: 78000 },
    });
    console.log(`✅ Order updated: cancelled ${testOrder.id}, created ${updatedOrderIds[0]}\n`);

    // Wait a moment
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Fetch orders again
    orders = await exchange.fetchOpenOrders();
    const updatedOrder = orders.find(o => o.id === updatedOrderIds[0]);
    
    if (updatedOrder) {
      console.log(`New order details: ${updatedOrder.symbol} ${updatedOrder.side} ${updatedOrder.amount} @ $${updatedOrder.price}\n`);
    }

    // Test 3: Cancel the updated order
    console.log('5. Cancelling the updated order...');
    if (updatedOrder) {
      await exchange.cancelOrder(updatedOrder);
      console.log(`✅ Order ${updatedOrder.id} cancelled\n`);
    }

    // Test 4: Check final order count
    console.log('6. Fetching final order count...');
    orders = await exchange.fetchOpenOrders();
    console.log(`✅ Final order count: ${orders.length}\n`);

    console.log('✅ All tests completed successfully!\n');
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.stack) {
      console.error('Stack:', err.stack);
    }
  } finally {
    exchange.dispose();
  }
}

testCancelAndUpdate().catch(console.error);
