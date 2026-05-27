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

  console.log('Testing /api/v1/capital endpoint...\n');

  const authHeaders = await generateAuthHeaders('balanceQuery', {}, creds.apiKey, creds.secret);

  try {
    const response = await axios.get('https://api.backpack.exchange/api/v1/capital', {
      headers: {
        'Content-Type': 'application/json',
        ...authHeaders,
      },
    });

    console.log('Status:', response.status);
    console.log('Response type:', typeof response.data);
    console.log('Is array:', Array.isArray(response.data));
    console.log('Response data:', JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.error('Error:', error.response?.data || error.message);
  }
}

main();
