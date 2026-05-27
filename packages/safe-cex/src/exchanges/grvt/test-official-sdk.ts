#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { GrvtClient } from '@tealstreet/grvt-ts-sdk';
import { EGrvtEnvironment } from '@tealstreet/grvt-ts-sdk';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testOfficialSDK() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Official SDK Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('Signer Address:', config.grvtSignerAddress);
  console.log('==============================\n');

  try {
    // Initialize the SDK client
    const client = new GrvtClient({
      apiKey: config.apiKey,
      apiSecret: config.secret,
      env: config.testnet ? EGrvtEnvironment.TESTNET : EGrvtEnvironment.PRODUCTION,
    });

    console.log('1. SDK Client initialized\n');

    // Test 1: Get sub-account summary (empty = default)
    console.log('2. Testing getSubAccountSummary with empty sub_account_id...');
    try {
      const summary = await client.getSubAccountSummary({
        sub_account_id: '',
      });

      console.log('✅ SUCCESS!');
      console.log('Response:', JSON.stringify(summary, null, 2));
    } catch (err: any) {
      console.error('❌ Failed:');
      console.error('  Message:', err.message);
      console.error('  Response:', err.response?.data);
    }
    console.log('\n');

    // Test 2: Get sub-account summary with specific ID
    if (config.grvtTradingAccountId) {
      console.log(`3. Testing getSubAccountSummary with sub_account_id: ${config.grvtTradingAccountId}...`);
      try {
        const summary = await client.getSubAccountSummary({
          sub_account_id: config.grvtTradingAccountId,
        });

        console.log('✅ SUCCESS!');
        console.log('Response:', JSON.stringify(summary, null, 2));
      } catch (err: any) {
        console.error('❌ Failed:');
        console.error('  Message:', err.message);
        console.error('  Response:', err.response?.data);
      }
      console.log('\n');
    }

    // Test 3: Get funding account summary
    console.log('4. Testing getFundingAccountSummary...');
    try {
      const fundingSummary = await client.getFundingAccountSummary();

      console.log('✅ SUCCESS!');
      console.log('Response:', JSON.stringify(fundingSummary, null, 2));
    } catch (err: any) {
      console.error('❌ Failed:');
      console.error('  Message:', err.message);
      console.error('  Response:', err.response?.data);
    }

  } catch (err: any) {
    console.error('\n❌ Unexpected Error:', err.message);
    if (err.stack) {
      console.error('Stack:', err.stack);
    }
  }

  console.log('\n=== Test Complete ===\n');
}

testOfficialSDK().catch(console.error);
