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

async function testPlaceOrder() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Place Order Test ===');
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

    // Find BTC market
    const btcMarket = markets.find((m) => m.symbol === 'BTC_USDT_Perp');
    if (!btcMarket) {
      throw new Error('BTC_USDT_Perp market not found');
    }
    console.log('BTC Market Info:');
    console.log(`  ID: ${btcMarket.id}`);
    console.log(`  Symbol: ${btcMarket.symbol}`);
    console.log(`  Precision - Price: ${btcMarket.precision.price}, Amount: ${btcMarket.precision.amount}`);
    console.log(`  Limits - Amount: ${btcMarket.limits.amount?.min} - ${btcMarket.limits.amount?.max}`);
    console.log(`  Limits - Leverage: ${btcMarket.limits.leverage?.min} - ${btcMarket.limits.leverage?.max}`);
    console.log(`  Raw info:`, JSON.stringify(btcMarket.info, null, 2));
    console.log();

    // Fetch current balance
    console.log('3. Fetching balance...');
    const balance = await exchange.fetchBalance();
    console.log(`Balance: $${balance.total.toFixed(2)}`);
    console.log(`Available: $${balance.free.toFixed(2)}\n`);

    // Place test order (note: may be rejected if price is outside protection bands)
    console.log('4. Placing BTC limit order...');
    console.log('  Symbol: BTC_USDT_Perp');
    console.log('  Side: Buy');
    console.log('  Amount: 0.001 BTC');
    console.log('  Price: $70,000');
    console.log('  Type: Limit');
    console.log('  Time In Force: GoodTillCancel\n');

    const orderIds = await exchange.placeOrder({
      symbol: 'BTC_USDT_Perp',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.001,
      price: 70000,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    console.log(`✅ Order placed successfully!`);
    console.log(`   Order ID: ${orderIds[0]}\n`);

    // Fetch orders to verify
    console.log('5. Fetching open orders to verify...');
    await new Promise((resolve) => setTimeout(resolve, 1000)); // Wait 1s
    const orders = await exchange.fetchOpenOrders();
    const newOrder = orders.find((o) => o.id === orderIds[0]);

    if (newOrder) {
      console.log(`✅ Order confirmed in open orders:`);
      console.log(`   ID: ${newOrder.id}`);
      console.log(`   Symbol: ${newOrder.symbol}`);
      console.log(`   Side: ${newOrder.side}`);
      console.log(`   Amount: ${newOrder.amount}`);
      console.log(`   Price: $${newOrder.price}`);
      console.log(`   Status: ${newOrder.status}\n`);
    } else {
      console.log(`⚠️ Order not found in open orders (may have been filled or rejected)\n`);
    }

    console.log('✅ Test completed successfully!\n');
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

testPlaceOrder().catch(console.error);
