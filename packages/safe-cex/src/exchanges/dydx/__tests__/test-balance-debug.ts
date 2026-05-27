import { readFileSync } from 'fs';
import { join } from 'path';
import { DydxExchange } from '../dydx.exchange';
import { ExchangeOptions } from '../../../types';
import { DefaultStore } from '../../../store/store.base';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

async function main() {
  console.log('🔍 dYdX Balance Debug Test\n');
  console.log('━'.repeat(50));

  // Load API credentials
  const configPath = join(__dirname, '..', '.apikey.json');
  let apiConfig: ApiKeyConfig;

  try {
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);
    console.log('\n✅ API credentials loaded');
    console.log('   Address:', apiConfig.apiKey);
    console.log('   Testnet:', apiConfig.testnet || false);
  } catch (error) {
    console.error('❌ Failed to load API credentials:', error);
    process.exit(1);
  }

  // Create exchange options
  const options: ExchangeOptions = {
    key: apiConfig.apiKey,
    secret: apiConfig.secret,
    testnet: apiConfig.testnet || false,
  };

  // Create store and exchange
  const store = new DefaultStore();
  const exchange = new DydxExchange('dydx', options, store);

  console.log('\n📡 Validating account...');
  const validationResult = await exchange.validateAccount();

  if (validationResult) {
    console.error('❌ Validation failed:', validationResult);
    process.exit(1);
  }

  console.log('✅ Account validated\n');

  // Fetch balance
  console.log('💰 Fetching balance...');
  const balance = await exchange.fetchBalance();
  console.log('\n📊 Balance Data:');
  console.log(JSON.stringify(balance, null, 2));

  // Fetch positions
  console.log('\n📈 Fetching positions...');
  const positions = await exchange.fetchPositions();
  console.log('\n📊 Positions Data:');
  console.log(JSON.stringify(positions, null, 2));
  console.log(`   Total positions: ${positions.length}`);

  // Fetch open orders
  console.log('\n📋 Fetching open orders...');
  const orders = await exchange.fetchOpenOrders();
  console.log('\n📊 Orders Data:');
  console.log(JSON.stringify(orders, null, 2));
  console.log(`   Total open orders: ${orders.length}`);

  console.log('\n' + '━'.repeat(50));
  console.log('✅ Test complete!\n');

  exchange.dispose();
}

main().catch((error) => {
  console.error('\n❌ Error:', error);
  process.exit(1);
});
