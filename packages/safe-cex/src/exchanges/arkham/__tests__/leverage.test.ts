/**
 * Test script for Arkham leverage endpoints
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-leverage.ts
 */

import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

const credentialsPath = path.join(__dirname, '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;
const BASE_URL = 'https://arkm.com/api';

function createSignature(
  apiKey: string,
  apiSecret: string,
  method: string,
  path: string,
  body: string = ''
): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000;
  const expiry = timestamp + 5 * 1000000;
  const message = apiKey + expiry + method + path + body;
  const decodedSecret = Buffer.from(apiSecret, 'base64');
  const hexSecret = decodedSecret.toString('hex');
  const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
    .update(message)
    .digest('base64');
  return { signature, expiry };
}

async function testLeverageEndpoints() {
  console.log('Testing Arkham Leverage Endpoints\n');

  // Test 1: GET /account/leverage
  console.log('Test 1: GET /account/leverage');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const endpoint = '/account/leverage';
    const method = 'GET';
    const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

    const response = await axios.get(BASE_URL + endpoint, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
      },
    });

    console.log('Success!');
    console.log('Response:', JSON.stringify(response.data, null, 2));
    console.log();
  } catch (error: any) {
    console.log('Failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message);
    console.log();
  }

  // Test 2: Check position response for margin type info
  console.log('Test 2: GET /account/positions (check for margin type)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const endpoint = '/account/positions';
    const method = 'GET';
    const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint);

    const response = await axios.get(BASE_URL + endpoint, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
      },
    });

    console.log('Success!');
    if (response.data.positions && response.data.positions.length > 0) {
      console.log('Sample position fields:', Object.keys(response.data.positions[0]));
      console.log('First position:', JSON.stringify(response.data.positions[0], null, 2));
    } else {
      console.log('No positions found');
    }
    console.log();
  } catch (error: any) {
    console.log('Failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message);
    console.log();
  }
}

testLeverageEndpoints().catch((err) => {
  console.error('Test error:', err);
  process.exit(1);
});
