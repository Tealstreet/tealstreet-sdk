/**
 * Direct API test to diagnose 403 errors
 */

import axios from 'axios';
import { readFileSync } from 'fs';
import { join } from 'path';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet: boolean;
}

async function testDirectAPI() {
  console.log('\n===========================================');
  console.log('🔍 dYdX Direct API Test');
  console.log('===========================================\n');

  // Load credentials
  const configPath = join(__dirname, '..', '.apikey.json');
  const config: ApiKeyConfig = JSON.parse(readFileSync(configPath, 'utf-8'));

  const baseURL = config.testnet
    ? 'https://indexer.v4testnet.dydx.exchange'
    : 'https://indexer.dydx.trade';

  const address = config.apiKey;
  const subaccount = 0;

  console.log('📋 Configuration:');
  console.log(`  Base URL: ${baseURL}`);
  console.log(`  Address: ${address}`);
  console.log(`  Subaccount: ${subaccount}\n`);

  try {
    // Test 1: Get server height (public endpoint)
    console.log('1️⃣ Testing public endpoint (height)...');
    const heightResponse = await axios.get(`${baseURL}/v4/height`);
    console.log(`✅ Success! Current height: ${heightResponse.data.height}`);
    console.log(`   Block time: ${heightResponse.data.time}\n`);

  } catch (error: any) {
    console.error('❌ Failed to get height:');
    console.error(`   Status: ${error.response?.status}`);
    console.error(`   Message: ${error.message}\n`);
  }

  try {
    // Test 2: Get markets (public endpoint)
    console.log('2️⃣ Testing markets endpoint...');
    const marketsResponse = await axios.get(`${baseURL}/v4/perpetualMarkets`);
    const marketCount = Object.keys(marketsResponse.data.markets || {}).length;
    console.log(`✅ Success! Found ${marketCount} markets\n`);

  } catch (error: any) {
    console.error('❌ Failed to get markets:');
    console.error(`   Status: ${error.response?.status}`);
    console.error(`   Message: ${error.message}\n`);
  }

  try {
    // Test 3: Get subaccount (private endpoint - may not exist)
    console.log('3️⃣ Testing subaccount endpoint...');
    const url = `${baseURL}/v4/addresses/${address}/subaccountNumber/${subaccount}`;
    console.log(`   URL: ${url}`);

    const subaccountResponse = await axios.get(url);
    console.log(`✅ Success! Subaccount exists`);
    console.log(`   Equity: ${subaccountResponse.data?.subaccount?.equity || 0}\n`);

  } catch (error: any) {
    if (error.response?.status === 404) {
      console.log(`ℹ️  Subaccount not found (404) - This is normal for empty accounts`);
      console.log(`   The account has never deposited or traded on dYdX\n`);
    } else if (error.response?.status === 403) {
      console.error('❌ 403 Forbidden - Access denied');
      console.error(`   This might indicate:`);
      console.error(`   - IP restrictions on the API`);
      console.error(`   - Region blocking (some endpoints may be geo-restricted)`);
      console.error(`   - Rate limiting`);
      console.error(`   - CORS issues (if running in browser)`);
      console.error(`   Response data:`, JSON.stringify(error.response?.data, null, 2));
      console.error(`\n`);
    } else {
      console.error(`❌ Failed with status ${error.response?.status}:`);
      console.error(`   Message: ${error.message}`);
      console.error(`   Response:`, JSON.stringify(error.response?.data, null, 2));
      console.error(`\n`);
    }
  }

  try {
    // Test 4: Try asset positions
    console.log('4️⃣ Testing asset positions endpoint...');
    const url = `${baseURL}/v4/assetPositions?address=${address}&subaccountNumber=${subaccount}`;
    console.log(`   URL: ${url}`);

    const response = await axios.get(url);
    console.log(`✅ Success! Positions:`, JSON.stringify(response.data, null, 2).substring(0, 200) + '...\n');

  } catch (error: any) {
    if (error.response?.status === 404) {
      console.log(`ℹ️  No asset positions found (404) - Empty account\n`);
    } else if (error.response?.status === 403) {
      console.error('❌ 403 Forbidden');
      console.error(`   Response:`, JSON.stringify(error.response?.data, null, 2));
      console.error(`\n`);
    } else {
      console.error(`❌ Failed with status ${error.response?.status}:`);
      console.error(`   Message: ${error.message}\n`);
    }
  }

  console.log('===========================================\n');
}

testDirectAPI().catch(console.error);
