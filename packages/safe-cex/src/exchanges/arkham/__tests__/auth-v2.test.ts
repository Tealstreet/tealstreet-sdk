/**
 * Test script for Arkham authentication (CORRECTED)
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-auth-v2.ts
 */

import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

// Load API credentials
const credentialsPath = path.join(__dirname, '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;
const BASE_URL = 'https://arkm.com/api';

console.log('🔐 Testing Arkham Authentication (CORRECTED)\n');
console.log('API Key:', API_KEY);
console.log('Base URL:', BASE_URL, '\n');

/**
 * Create authentication signature for Arkham API
 * CORRECTED: Expiry in MICROSECONDS, not milliseconds
 */
function createSignature(
  apiKey: string,
  apiSecret: string,
  method: string,
  path: string,
  queryString: string = '',
  body: string = ''
): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000; // Convert to microseconds
  const expiry = timestamp + 5 * 1000000; // 5 seconds in microseconds

  // Arkham signature format: api_key + expiry + method + path + query + body
  const fullPath = queryString ? `${path}?${queryString}` : path;
  const message = `${apiKey}${expiry}${method}${fullPath}${body}`;

  console.log('📝 Signature Message:', message);
  console.log('📝 Expiry (microseconds):', expiry);
  console.log('📝 Current time (microseconds):', timestamp);

  const signature = createHmac('sha256', apiSecret).update(message).digest('hex');

  return { signature, expiry };
}

/**
 * Test 1: Validate account endpoint
 */
async function testAccountValidation() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 1: Account Validation');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/account/info';
  const method = 'GET';

  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Arkham-Api-Key': API_KEY, // Note: Capital 'A' in Api
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(), // Note: Expires, not Expiry
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Account validation successful!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return true;
  } catch (error: any) {
    console.log('❌ Account validation failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return false;
  }
}

/**
 * Test 2: Fetch markets
 */
async function testFetchMarkets() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 2: Fetch Markets (Public Endpoint)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/public/markets';

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Markets fetch successful!');
    console.log(`Found ${response.data.markets?.length || 0} markets`);
    if (response.data.markets?.length > 0) {
      console.log('First market:', JSON.stringify(response.data.markets[0], null, 2));
    }
    console.log();
    return true;
  } catch (error: any) {
    console.log('❌ Markets fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return false;
  }
}

/**
 * Test 3: Fetch balances
 */
async function testFetchBalances() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 3: Fetch Balances');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/account/balances';
  const method = 'GET';

  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Balances fetch successful!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return true;
  } catch (error: any) {
    console.log('❌ Balances fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return false;
  }
}

/**
 * Test 4: Fetch positions
 */
async function testFetchPositions() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 4: Fetch Positions');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/account/positions';
  const method = 'GET';

  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Positions fetch successful!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return true;
  } catch (error: any) {
    console.log('❌ Positions fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return false;
  }
}

/**
 * Test 5: Fetch active orders
 */
async function testFetchOrders() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 5: Fetch Active Orders');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/orders/active';
  const method = 'GET';

  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Orders fetch successful!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return true;
  } catch (error: any) {
    console.log('❌ Orders fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return false;
  }
}

/**
 * Run all tests
 */
async function runAllTests() {
  console.log('🚀 Starting Arkham API Tests\n');

  const results = {
    accountValidation: await testAccountValidation(),
    fetchMarkets: await testFetchMarkets(),
    fetchBalances: await testFetchBalances(),
    fetchPositions: await testFetchPositions(),
    fetchOrders: await testFetchOrders(),
  };

  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📊 Test Results Summary');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  Object.entries(results).forEach(([test, passed]) => {
    console.log(`${passed ? '✅' : '❌'} ${test}`);
  });

  const totalTests = Object.keys(results).length;
  const passedTests = Object.values(results).filter((r) => r).length;

  console.log(`\n${passedTests}/${totalTests} tests passed\n`);

  if (passedTests === totalTests) {
    console.log('🎉 All tests passed! Authentication is working correctly.\n');
  } else {
    console.log('⚠️  Some tests failed. Please review the errors above.\n');
  }
}

// Run tests
runAllTests().catch(console.error);
