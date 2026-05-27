/**
 * Test script for Arkham authentication (FINAL CORRECTED VERSION)
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-auth-v3.ts
 */

import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

// Load API credentials
const credentialsPath = path.join(__dirname, '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret; // This is base64-encoded
const BASE_URL = 'https://arkm.com/api';

console.log('🔐 Testing Arkham Authentication (FINAL CORRECTED)\n');
console.log('API Key:', API_KEY);
console.log('API Secret (base64):', API_SECRET);
console.log('Base URL:', BASE_URL, '\n');

/**
 * Create authentication signature for Arkham API
 * CORRECTED PROCESS:
 * 1. Decode secret from base64
 * 2. Convert decoded secret to hex
 * 3. Use hex as HMAC key
 * 4. Encode result as base64 (not hex!)
 */
function createSignature(
  apiKey: string,
  apiSecret: string, // base64-encoded secret
  method: string,
  path: string,
  body: string = ''
): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000; // Convert to microseconds
  const expiry = timestamp + 5 * 1000000; // 5 seconds in microseconds

  // Arkham signature format: api_key + expiry + method + path + body
  const message = `${apiKey}${expiry}${method}${path}${body}`;

  console.log('📝 Signature Message:', message);

  // Step 1: Decode secret from base64
  const decodedSecret = Buffer.from(apiSecret, 'base64');

  // Step 2: Convert to hex
  const hexSecret = decodedSecret.toString('hex');

  console.log('📝 Secret (hex):', hexSecret);

  // Step 3 & 4: Create HMAC with hex secret, output as base64
  const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
    .update(message)
    .digest('base64');

  console.log('📝 Signature (base64):', signature);
  console.log('📝 Expiry (microseconds):', expiry);

  return { signature, expiry };
}

/**
 * Test 1: Get user info
 */
async function testUserInfo() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 1: Get User Info');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/user';
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

    console.log('✅ User info fetch successful!');
    console.log('Response:', JSON.stringify(response.data, null, 2), '\n');
    return true;
  } catch (error: any) {
    console.log('❌ User info fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return false;
  }
}

/**
 * Test 2: Fetch trading pairs
 */
async function testFetchPairs() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 2: Fetch Trading Pairs (Public)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  const endpoint = '/public/pairs';

  try {
    const response = await axios.get(`${BASE_URL}${endpoint}`, {
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Pairs fetch successful!');
    console.log(`Found ${response.data.pairs?.length || response.data.length || 0} pairs`);
    if (response.data.pairs && response.data.pairs.length > 0) {
      console.log('First pair:', JSON.stringify(response.data.pairs[0], null, 2));
    } else if (Array.isArray(response.data) && response.data.length > 0) {
      console.log('First pair:', JSON.stringify(response.data[0], null, 2));
    }
    console.log();
    return true;
  } catch (error: any) {
    console.log('❌ Pairs fetch failed!');
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

  const endpoint = '/orders';
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
    userInfo: await testUserInfo(),
    fetchPairs: await testFetchPairs(),
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
