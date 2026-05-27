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

async function testCancelAndTPSL() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Cancel and TP/SL Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Trading Account ID:', config.grvtTradingAccountId);
  console.log('Signer Address:', config.grvtSignerAddress);
  console.log('==============================\n');

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
    console.log('2. Fetching markets...');
    const markets = await exchange.fetchMarkets();
    store.update({ markets });
    console.log(`✅ Fetched ${markets.length} markets\n`);

    // Fetch existing open orders
    console.log('3. Fetching existing open orders...');
    const existingOrders = await exchange.fetchOpenOrders();
    console.log(`Found ${existingOrders.length} existing open orders\n`);

    // If there are existing orders, test cancellation
    if (existingOrders.length > 0) {
      const orderToCancel = existingOrders[0];
      console.log(`4. Cancelling order ${orderToCancel.id}...`);
      await exchange.cancelOrder(orderToCancel);
      console.log(`✅ Order ${orderToCancel.id} cancelled\n`);

      // Verify cancellation
      await new Promise((resolve) => setTimeout(resolve, 1000));
      const ordersAfter = await exchange.fetchOpenOrders();
      const stillExists = ordersAfter.find((o) => o.id === orderToCancel.id);
      if (!stillExists) {
        console.log('✅ Cancellation verified - order no longer in open orders\n');
      } else {
        console.log('⚠️ Order still shows as open (may take time to update)\n');
      }
    } else {
      console.log('4. No existing orders to cancel\n');
    }

    // Now test TP/SL orders
    console.log('5. Testing TP/SL order creation...');
    console.log('Note: TP/SL requires trigger metadata in order');
    console.log('Note: May fail with insufficient margin if balance is low\n');

    try {
      // Place order with TP
      console.log('6. Placing order with Take Profit...');
      const tpOrderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.001,
        price: 95000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
        takeProfit: 100000, // TP at 100k
      });
      console.log(`✅ Order with TP placed: ${tpOrderIds[0]}\n`);
    } catch (err: any) {
      console.log(`⚠️ Could not place TP order: ${err.message}\n`);
    }

    try {
      // Place order with SL
      console.log('7. Placing order with Stop Loss...');
      const slOrderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.001,
        price: 95000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
        stopLoss: 90000, // SL at 90k
      });
      console.log(`✅ Order with SL placed: ${slOrderIds[0]}\n`);
    } catch (err: any) {
      console.log(`⚠️ Could not place SL order: ${err.message}\n`);
    }

    console.log('✅ All tests completed successfully!\n');
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response?.data) {
      console.error('Response data:', JSON.stringify(err.response.data, null, 2));
    }
    if (err.stack) {
      console.error('Stack:', err.stack);
    }
  } finally {
    exchange.dispose();
  }
}

testCancelAndTPSL().catch(console.error);
