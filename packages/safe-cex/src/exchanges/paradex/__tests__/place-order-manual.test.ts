/**
 * Manual Test Script for Paradex Place Order
 *
 * This script tests the place order functionality with real API calls.
 * Run with: npx tsx packages/safe-cex/src/exchanges/paradex/test-place-order-manual.ts
 */

import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderSide, OrderType } from '../../../types';
import * as fs from 'fs';
import * as path from 'path';

async function main() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📝 Paradex Place Order Manual Test');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  // Load credentials
  const credsPath = path.join(__dirname, '.apikey.json');
  if (!fs.existsSync(credsPath)) {
    console.error('❌ .apikey.json not found');
    process.exit(1);
  }

  const testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
  console.log('✓ Loaded credentials from .apikey.json');
  console.log(`  Account: ${testCreds.paradexAddress}`);
  console.log(`  Subkey: ${testCreds.subkeyAddress}`);
  console.log(`  Testnet: ${testCreds.testnet}\n`);

  // Create store and exchange
  const store = new DefaultStore();
  const exchange = new ParadexExchange(
    'paradex',
    {
      key: testCreds.apiKey,
      secret: '', // Not used for Paradex
      testnet: testCreds.testnet || false,
      paradexAddress: testCreds.paradexAddress,
      subkeyAddress: testCreds.subkeyAddress,
    },
    store,
  );

  // Listen for errors
  exchange.on('error', (error: string) => {
    console.log('❌ Error event:', error);
  });

  try {
    // Wait for SDK initialization
    console.log('🔄 Initializing Paradex SDK...');
    if (exchange['sdkInitPromise']) {
      await exchange['sdkInitPromise'];
    }
    console.log('✓ SDK initialized\n');

    // Check SDK components
    console.log('🔍 Checking SDK components:');
    console.log(`  sdkAccount: ${exchange.sdkAccount ? '✓' : '✗'}`);
    console.log(`  requestSigner: ${exchange.requestSigner ? '✓' : '✗'}`);
    console.log(`  jwtManager: ${exchange.jwtManager ? '✓' : '✗'}`);

    if (exchange.sdkAccount) {
      console.log(`  Account address: ${exchange.sdkAccount.account.address}`);
    }
    if (exchange.requestSigner) {
      console.log(`  Signer account: ${exchange.requestSigner.getAccountAddress()}`);
    }
    console.log();

    // Fetch markets first
    console.log('🔄 Fetching markets...');
    const markets = await exchange.fetchMarkets();
    console.log(`✓ Fetched ${markets.length} markets\n`);

    store.update({
      markets,
      loaded: { ...store.loaded, markets: true },
    });

    // Attempt to place a limit order
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📝 Placing test order...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    console.log('Order details:');
    console.log('  Symbol: ETH-USD-PERP');
    console.log('  Side: Buy');
    console.log('  Type: Limit');
    console.log('  Amount: 0.01 ETH');
    console.log('  Price: $100 (far below market to avoid execution)');
    console.log();

    const orderIds = await exchange.placeOrder({
      symbol: 'ETH-USD-PERP',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.01,
      price: 100, // Far below market
    });

    console.log('✅ ORDER PLACED SUCCESSFULLY!');
    console.log(`   Order IDs: ${orderIds.join(', ')}\n`);

    // Clean up - cancel the order
    if (orderIds.length > 0) {
      console.log('🧹 Cleaning up - canceling test order...');
      await exchange.cancelOrder(orderIds[0]);
      console.log('✓ Order canceled\n');
    }

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('✅ ALL TESTS PASSED!');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');

  } catch (error: any) {
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('❌ TEST FAILED!');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.error('Error:', error.message);

    if (error.response) {
      console.error('\nAPI Response:');
      console.error('  Status:', error.response.status);
      console.error('  Data:', JSON.stringify(error.response.data, null, 2));
    }

    if (error.stack) {
      console.error('\nStack trace:');
      console.error(error.stack);
    }

    process.exit(1);
  } finally {
    // Dispose exchange
    if (exchange) {
      exchange.dispose();
    }
  }
}

main().catch((error) => {
  console.error('Unhandled error:', error);
  process.exit(1);
});
