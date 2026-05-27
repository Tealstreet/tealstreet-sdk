import * as fs from 'fs';
import * as path from 'path';

import * as ed25519 from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2.js';
import axios from 'axios';

const sha512ForEd25519: NonNullable<typeof ed25519.hashes.sha512> = (message) =>
  Uint8Array.from(sha512(message));

ed25519.hashes.sha512 = sha512ForEd25519;

async function generateAuthHeaders(
  instruction: string,
  params: Record<string, any>,
  apiKey: string,
  secret: string,
  window: number = 5000,
) {
  const timestamp = Date.now();

  const sortedKeys = Object.keys(params).sort();
  const queryParts: string[] = [];

  for (const key of sortedKeys) {
    const value = params[key];
    if (value !== null && value !== undefined) {
      queryParts.push(`${key}=${encodeURIComponent(String(value))}`);
    }
  }

  const queryString = queryParts.join('&');
  const signaturePayload = `instruction=${instruction}${queryString ? '&' + queryString : ''}&timestamp=${timestamp}&window=${window}`;

  console.log('Signature payload:', signaturePayload);

  const messageBytes = new TextEncoder().encode(signaturePayload);
  const privateKeyBytes = Uint8Array.from(Buffer.from(secret, 'base64'));
  const signatureBytes = await ed25519.signAsync(messageBytes, privateKeyBytes);
  const signature = Buffer.from(signatureBytes).toString('base64');

  return {
    'X-API-Key': apiKey,
    'X-Signature': signature,
    'X-Timestamp': timestamp.toString(),
    'X-Window': window.toString(),
  };
}

async function main() {
  const credsPath = path.join(__dirname, '.apikey.json');
  const creds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));

  console.log('Testing order placement...\n');

  // First get current price
  const tickerResponse = await axios.get('https://api.backpack.exchange/api/v1/ticker?symbol=SOL_USDC');
  const currentPrice = parseFloat(tickerResponse.data.lastPrice);
  console.log(`Current SOL price: $${currentPrice}\n`);

  // Place order at 95% of market (closer to avoid rejection)
  const orderPrice = (currentPrice * 0.95).toFixed(2);

  const orderRequest = {
    symbol: 'SOL_USDC',
    side: 'Bid',
    orderType: 'Limit',
    price: orderPrice,
    quantity: '0.01',
    timeInForce: 'GTC',
  };

  console.log('Order request:', JSON.stringify(orderRequest, null, 2));

  const authHeaders = await generateAuthHeaders('orderExecute', orderRequest, creds.apiKey, creds.secret);

  try {
    const response = await axios.post('https://api.backpack.exchange/api/v1/order', orderRequest, {
      headers: {
        'Content-Type': 'application/json',
        ...authHeaders,
      },
    });

    console.log('\n✅ Success!');
    console.log('Status:', response.status);
    console.log('Response:', JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.error('\n❌ Error:', error.response?.status);
    console.error('Error data:', JSON.stringify(error.response?.data, null, 2));
    console.error('Error message:', error.message);
  }
}

main();
