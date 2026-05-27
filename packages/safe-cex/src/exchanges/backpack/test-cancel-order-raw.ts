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

  const orderId = '15397552865'; // Order ID from previous test

  console.log(`Testing order cancellation for order ${orderId}...\n`);

  const cancelParams = {
    symbol: 'SOL_USDC',
    orderId: orderId,
  };

  console.log('Cancel request:', JSON.stringify(cancelParams, null, 2));

  const authHeaders = await generateAuthHeaders('orderCancel', cancelParams, creds.apiKey, creds.secret);

  try {
    const response = await axios.delete('https://api.backpack.exchange/api/v1/order', {
      data: cancelParams, // Send in body, not params
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
