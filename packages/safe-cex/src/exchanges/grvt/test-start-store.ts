#!/usr/bin/env tsx
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import * as fs from 'fs';
import * as path from 'path';

// Read API credentials from .apikey.json
const apiKeyPath = path.join(__dirname, '.apikey.json');
if (!fs.existsSync(apiKeyPath)) {
  console.error('❌ .apikey.json not found. Please create it with your GRVT credentials.');
  process.exit(1);
}

const credentials = JSON.parse(fs.readFileSync(apiKeyPath, 'utf-8'));

console.log('=== GRVT Store Test ===');
console.log('API Key:', credentials.apiKey.substring(0, 15) + '...');
console.log('Testnet:', credentials.testnet);
console.log('SubAccount ID:', credentials.subAccountId || '(default)');
console.log('===========================\n');

const store = new DefaultStore();
const exchange = new GrvtExchange(
  'grvt',
  {
    key: credentials.apiKey,
    secret: credentials.secret,
    testnet: credentials.testnet,
    subaccount: credentials.subAccountId,
    grvtSignerAddress: credentials.grvtSignerAddress,
    grvtTradingAccountId: credentials.grvtTradingAccountId,
  },
  store
);

async function testStorePopulation() {
  try {
    console.log('1. Calling exchange.start()...');
    await exchange.start();
    console.log('✅ start() completed!\n');

    // Wait a bit for async operations
    await new Promise((resolve) => setTimeout(resolve, 1000));

    console.log('2. Checking store.loaded flags:');
    const loaded = exchange.store.loaded;
    console.log('   markets:', loaded.markets ? '✅' : '❌');
    console.log('   tickers:', loaded.tickers ? '✅' : '❌');
    console.log('   balance:', loaded.balance ? '✅' : '❌');
    console.log('   positions:', loaded.positions ? '✅' : '❌');
    console.log('   orders:', loaded.orders ? '✅' : '❌');
    console.log('');

    console.log('3. Checking store data:');
    console.log('   markets count:', exchange.store.markets.length);
    console.log('   tickers count:', Object.keys(exchange.store.tickers).length);
    console.log('   balance total:', exchange.store.balance.total);
    console.log('   positions count:', exchange.store.positions.length);
    console.log('   orders count:', exchange.store.orders.length);
    console.log('');

    // Show some sample markets
    if (exchange.store.markets.length > 0) {
      console.log('4. Sample markets (first 5):');
      exchange.store.markets.slice(0, 5).forEach((m) => {
        console.log(`   ${m.symbol} - ${m.base}/${m.quote}`);
      });
      console.log('');
    }

    // Check if all flags are true
    const allLoaded =
      loaded.markets && loaded.tickers && loaded.balance && loaded.positions && loaded.orders;

    if (allLoaded) {
      console.log('✅ All store loaded flags are set!');
    } else {
      console.log('❌ Some store loaded flags are missing!');
      process.exit(1);
    }

    // Dispose
    await exchange.dispose();
    console.log('\n✅ Test completed successfully!');
    process.exit(0);
  } catch (err) {
    console.error('❌ Test failed:', err);
    process.exit(1);
  }
}

testStorePopulation();
