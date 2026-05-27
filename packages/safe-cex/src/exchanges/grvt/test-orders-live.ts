#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testOrders() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Orders Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('Trading Account ID:', config.grvtTradingAccountId || '(not set)');
  console.log('===========================\n');

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
    console.log('✅ Authenticated successfully!\n');

    console.log('2. Fetching open orders...');
    const orders = await exchange.fetchOpenOrders();

    console.log(`✅ Fetched ${orders.length} open order(s)\n`);

    if (orders.length === 0) {
      console.log('No open orders found.');
    } else {
      for (const order of orders) {
        console.log(`Order: ${order.symbol}`);
        console.log(`  ID: ${order.id}`);
        console.log(`  Type: ${order.type}`);
        console.log(`  Side: ${order.side}`);
        console.log(`  Price: $${order.price}`);
        console.log(`  Amount: ${order.amount}`);
        console.log(`  Filled: ${order.filled}`);
        console.log(`  Remaining: ${order.remaining}`);
        console.log(`  Status: ${order.status}`);
        console.log(`  Reduce Only: ${order.reduceOnly}`);
        console.log('');
      }
    }

    console.log('✅ Test completed successfully!\n');
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response) {
      console.error('Status:', err.response.status);
      console.error('Data:', err.response.data);
    }
    if (err.stack) {
      console.error('Stack:', err.stack);
    }
  } finally {
    exchange.dispose();
  }
}

testOrders().catch(console.error);
