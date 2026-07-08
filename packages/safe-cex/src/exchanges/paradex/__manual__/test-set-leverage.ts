import { readFileSync } from 'fs';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

const test = async () => {
  console.log('🔍 Testing Paradex Set Leverage for BTC-USD-PERP...\n');

  const credentialsPath = __dirname + '/.apikey.json';
  let credentials: any;

  try {
    const credentialsData = readFileSync(credentialsPath, 'utf-8');
    credentials = JSON.parse(credentialsData);
    console.log('✅ Loaded credentials from .apikey.json');
  } catch (error) {
    console.error('❌ Error reading .apikey.json:', error);
    process.exit(1);
  }

  const options: ExchangeOptions = {
    key: credentials.apiKey,
    secret: '',
    paradexAddress: credentials.paradexAddress,
    subkeyAddress: credentials.subkeyAddress,
    testnet: credentials.testnet || false,
  };

  const store = new DefaultStore();
  const exchange = new ParadexExchange('paradex', options, store);

  try {
    console.log('🚀 Starting exchange...');
    await exchange.start();
    console.log('✅ Exchange started successfully\n');

    // Wait a bit for markets and positions to load
    await new Promise((resolve) => setTimeout(resolve, 3000));

    const symbol = 'BTC-USD-PERP';

    // Check if market exists
    const market = exchange.store.markets.find((m) => m.symbol === symbol);
    if (!market) {
      console.error(`❌ Market ${symbol} not found`);
      console.log('Available markets:', exchange.store.markets.map(m => m.symbol).slice(0, 10));
      return;
    }

    console.log(`✅ Market found: ${symbol}`);
    console.log(`   Leverage limits: min=${market.limits.leverage?.min}, max=${market.limits.leverage?.max}`);

    // Check if position exists
    const position = exchange.store.positions.find((p) => p.symbol === symbol);
    if (position) {
      console.log(`\n📊 Current Position:`);
      console.log(`   Symbol: ${position.symbol}`);
      console.log(`   Side: ${position.side}`);
      console.log(`   Contracts: ${position.contracts}`);
      console.log(`   Leverage: ${position.leverage}x`);
    } else {
      console.log(`\n⚠️  No open position found for ${symbol}`);
    }

    // Try to set leverage
    console.log(`\n🔧 Attempting to set leverage to 10x...`);

    try {
      await exchange.setLeverage(symbol, 10);
      console.log('✅ Leverage set successfully!');
    } catch (err: any) {
      console.error('\n❌ Error setting leverage:');
      console.error('   Error name:', err.name);
      console.error('   Error constructor:', err.constructor.name);
      console.error('   Message:', err.message);
      console.error('   Response status:', err.response?.status);
      console.error('   Response statusText:', err.response?.statusText);
      console.error('   Response data:', JSON.stringify(err.response?.data, null, 2));
      console.error('   Request URL:', err.config?.url);
      console.error('   Request method:', err.config?.method?.toUpperCase());
      console.error('   Request data:', err.config?.data);
      console.error('   Full error:', JSON.stringify(err, Object.getOwnPropertyNames(err), 2));
    }

    // Try with different leverage values
    console.log(`\n🔧 Attempting to set leverage to 5x...`);
    try {
      await exchange.setLeverage(symbol, 5);
      console.log('✅ Leverage set to 5x successfully!');
    } catch (err: any) {
      console.error('❌ Failed to set leverage to 5x');
      console.error('   Error:', err.response?.data || err.message);
    }

  } catch (error: any) {
    console.error('\n❌ Test failed:', error.message);
    console.error(error.stack);
  } finally {
    console.log('\n🛑 Disposing exchange...');
    exchange.dispose();
    process.exit(0);
  }
};

test();
