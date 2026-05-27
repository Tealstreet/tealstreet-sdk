/**
 * Test script to generate a valid curl command for Paradex authentication
 * Uses credentials from .apikey.json
 */
import { Account } from 'starknet';
import * as fs from 'fs';
import * as path from 'path';

// Load credentials
const credsPath = path.join(__dirname, '.apikey.json');
const creds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));

const subkeyAddress = creds.subkeyAddress;
const mainAccountAddress = creds.paradexAddress;
const privateKey = creds.apiKey;

// Build timestamp and expiry (5 minutes from now)
const timestamp = Math.floor(Date.now() / 1000);
const expiry = timestamp + 5 * 60; // 5 minutes from now
const method = 'POST';
const path_str = '/v1/auth'; // NOT /v1/auth/{subkey} - just /v1/auth
const body = '';

console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
console.log('Paradex Authentication Test');
console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
console.log('Subkey Address:', subkeyAddress);
console.log('Main Account:', mainAccountAddress);
console.log('Timestamp:', timestamp);
console.log('Expiry:', expiry);
console.log('Method:', method);
console.log('Path:', path_str);
console.log('Body:', body || '(empty)');

// Create account using MAIN ACCOUNT address but SUBKEY private key
// This matches paradex-py's SubkeyAccount: address=l2_address (main), key_pair from l2_private_key (subkey)
const account = new Account(
  { nodeUrl: 'https://api.prod.paradex.trade' },
  mainAccountAddress, // MAIN account address
  privateKey  // SUBKEY private key
);

console.log('\nAccount details:');
console.log('- Account address:', account.address);
console.log('- Private key:', privateKey.substring(0, 10) + '...');

const chainId = 'PRIVATE_SN_PARACLEAR_MAINNET'; // "PARADEX CHAIN" in hex

(async () => {
  try {
    console.log('\nSigning message with correct TypedData structure...');

    // Use the EXACT structure from paradex-py auth.py
    // IMPORTANT: Field order in domain and types must match Python SDK exactly
    const signature = await account.signMessage({
      domain: {
        name: 'Paradex',
        chainId: chainId, // lowercase 'c' in chainId
        version: '1',
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'chainId', type: 'felt' }, // lowercase 'c'
          { name: 'version', type: 'felt' },
        ],
        Request: [
          { name: 'method', type: 'felt' },
          { name: 'path', type: 'felt' },
          { name: 'body', type: 'felt' },
          { name: 'timestamp', type: 'felt' },
          { name: 'expiration', type: 'felt' },
        ],
      },
      primaryType: 'Request',
      message: {
        method: method,
        path: path_str,
        body: body,
        timestamp: timestamp,
        expiration: expiry,
      },
    });

    const sigArray = Array.isArray(signature)
      ? signature.map((s: any) => (typeof s === 'bigint' ? s.toString() : String(s)))
      : [String((signature as any).r), String((signature as any).s)];

    console.log('✓ Signature generated:', JSON.stringify(sigArray));

    // Make curl command
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Curl Command:');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    
    const curlCmd = `curl -X POST 'https://api.prod.paradex.trade/v1/auth/${subkeyAddress}' \\
  -H 'Content-Type: application/json' \\
  -H 'PARADEX-STARKNET-ACCOUNT: ${mainAccountAddress}' \\
  -H 'PARADEX-STARKNET-SIGNATURE: ${JSON.stringify(sigArray)}' \\
  -H 'PARADEX-TIMESTAMP: ${timestamp}' \\
  -H 'PARADEX-SIGNATURE-EXPIRATION: ${expiry}'`;

    console.log(curlCmd);
    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  } catch (error) {
    console.error('✗ Error:', error);
  }
})();
