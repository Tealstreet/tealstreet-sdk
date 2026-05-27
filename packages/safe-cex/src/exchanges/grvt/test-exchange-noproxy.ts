/**
 * Test GRVT Exchange class authentication without proxy
 * Run with: npx tsx src/exchanges/grvt/test-exchange-noproxy.ts
 *
 * This test validates that the GrvtExchange class properly:
 * 1. Authenticates with GRVT API
 * 2. Fetches account data
 * 3. Works with the SDK integration
 *
 * Note: This bypasses the Tealstreet proxy which requires browser auth
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import { Wallet } from 'ethers';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet: boolean;
  grvtSignerAddress?: string;
  grvtTradingAccountId?: string;
}

async function main() {
  console.log('=== GRVT Exchange Class Test (No Proxy) ===\n');

  // Load credentials
  const configPath = join(__dirname, '.apikey.json');
  let config: ApiKeyConfig;

  try {
    const configData = readFileSync(configPath, 'utf-8');
    config = JSON.parse(configData);
    console.log('Loaded credentials from .apikey.json');
    console.log(`  API Key: ${config.apiKey.substring(0, 10)}...`);
    console.log(`  Testnet: ${config.testnet}`);

    if (config.secret) {
      const wallet = new Wallet(config.secret);
      console.log(`  Signer Address: ${wallet.address}`);
    }
  } catch (err) {
    console.error('Failed to load .apikey.json:', err);
    return;
  }

  const store = new DefaultStore();

  // Create exchange WITHOUT proxy (tealstreetTunnel: false)
  console.log('\n--- Creating GrvtExchange (no proxy) ---');
  const exchange = new GrvtExchange('grvt', {
    key: config.apiKey,
    secret: config.secret,
    testnet: config.testnet ?? false,
    grvtTradingAccountId: config.grvtTradingAccountId,
    // Explicitly disable proxy for CLI testing
    tealstreetTunnel: false,
    tealstreetProxy: false,
  }, store);

  try {
    // Step 1: Test authenticate()
    console.log('\n--- Step 1: Testing authenticate() ---');
    const authResult = await exchange.authenticate();
    console.log('  authenticate() returned:', authResult);
    console.log('  exchange.apiExtras:', JSON.stringify(exchange.apiExtras, null, 2));

    if (!exchange.apiExtras?.sessionCookie) {
      console.log('  ERROR: No session cookie after authenticate()');
      return;
    }
    console.log('  Session cookie obtained successfully');

    // Step 2: Test tradingXhr with session
    console.log('\n--- Step 2: Testing tradingXhr (direct) ---');
    try {
      const response = await exchange.tradingXhr.post('/lite/v1/funding_account_summary', {});
      console.log('  tradingXhr SUCCESS');
      console.log('  Response:', JSON.stringify(response.data, null, 2).substring(0, 300));
    } catch (err: any) {
      console.log('  tradingXhr FAILED:', err.message);
    }

    // Step 3: Test SDK client
    console.log('\n--- Step 3: Testing grvtClient.getFundingAccountSummary() ---');
    try {
      const response = await exchange.grvtClient?.getFundingAccountSummary();
      console.log('  SDK SUCCESS');
      console.log('  Response:', JSON.stringify(response, null, 2).substring(0, 300));
    } catch (err: any) {
      console.log('  SDK FAILED:', err.message);
    }

    // Step 4: Test validateAccount
    console.log('\n--- Step 4: Testing validateAccount() ---');
    try {
      const result = await exchange.validateAccount();
      console.log('  validateAccount() returned:', result === '' ? '(empty - success)' : result);
    } catch (err: any) {
      console.log('  validateAccount() FAILED:', err.message);
    }

    // Step 5: Test fetchBalance
    console.log('\n--- Step 5: Testing fetchBalance() ---');
    try {
      const balance = await exchange.fetchBalance();
      console.log('  fetchBalance() SUCCESS');
      console.log('  Balance:', JSON.stringify(balance, null, 2).substring(0, 300));
    } catch (err: any) {
      console.log('  fetchBalance() FAILED:', err.message);
    }

    // Step 6: Test fetchPositions
    console.log('\n--- Step 6: Testing fetchPositions() ---');
    try {
      const positions = await exchange.fetchPositions();
      console.log('  fetchPositions() SUCCESS');
      console.log('  Positions count:', positions.length);
      if (positions.length > 0) {
        console.log('  First position:', JSON.stringify(positions[0], null, 2).substring(0, 300));
      }
    } catch (err: any) {
      console.log('  fetchPositions() FAILED:', err.message);
    }

    // Step 7: Test fetchOpenOrders
    console.log('\n--- Step 7: Testing fetchOpenOrders() ---');
    try {
      const orders = await exchange.fetchOpenOrders();
      console.log('  fetchOpenOrders() SUCCESS');
      console.log('  Open orders count:', orders.length);
    } catch (err: any) {
      console.log('  fetchOpenOrders() FAILED:', err.message);
      // Check if it's the 1001 error (signer not tagged)
      if (err.message?.includes('1001')) {
        console.log('\n  NOTE: Error 1001 means your signer address is not tagged to your API key.');
        console.log('  Go to GRVT UI → Account → API Keys and tag your address.');
      }
    }

    console.log('\n--- Test Complete ---');

  } catch (err: any) {
    console.error('Test failed:', err.message);
  } finally {
    exchange.dispose();
  }
}

main().catch(console.error);
