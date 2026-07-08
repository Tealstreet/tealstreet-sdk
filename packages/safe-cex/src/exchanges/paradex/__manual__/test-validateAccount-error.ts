/**
 * Paradex validateAccount Error Reproduction Script
 *
 * This script reproduces the INVALID_TOKEN error that occurs during validateAccount:
 * "INVALID_TOKEN: invalid bearer jwt: missing value in request header"
 *
 * ## Running this script
 *
 * ```bash
 * cd /home/rayana/projects/tealstreet-next
 * npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
 * ```
 *
 * ## What this script does
 *
 * 1. Demonstrates the error when calling /account without JWT
 * 2. Demonstrates the error with empty JWT
 * 3. Shows successful JWT generation and /account call with valid credentials
 */

import { Account } from 'starknet';
import * as fs from 'fs';
import * as path from 'path';

const BASE_URL = 'https://api.prod.paradex.trade/v1';

// Load credentials from .apikey.json
interface TestCredentials {
  exchange: string;
  testnet: boolean;
  name: string;
  apiKey: string;
  paradexAddress: string;
  ethereumAddress: string;
  subkeyAddress: string;
  user: string;
}

let testCreds: TestCredentials | null = null;
try {
  const credsPath = path.join(__dirname, '.apikey.json');
  testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
} catch (error) {
  console.error('❌ Error loading .apikey.json:', error);
  process.exit(1);
}

async function main() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Paradex validateAccount Error Reproduction');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('');
  console.log('Test credentials loaded:');
  console.log('  - Exchange:', testCreds!.exchange);
  console.log('  - Testnet:', testCreds!.testnet);
  console.log('  - Paradex Address:', testCreds!.paradexAddress.substring(0, 20) + '...');
  console.log('  - Subkey Address:', testCreds!.subkeyAddress.substring(0, 20) + '...');
  console.log('');

  // ========================================================================
  // TEST 1: Reproduce INVALID_TOKEN error (no JWT)
  // ========================================================================
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('TEST 1: Call /account WITHOUT JWT (should fail)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('');

  try {
    const response1 = await fetch(`${BASE_URL}/account`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
        // No Authorization header
      },
    });

    console.log('Response status:', response1.status, response1.statusText);
    const error1 = await response1.json();
    console.log('Error response:', JSON.stringify(error1, null, 2));
    console.log('');

    if (error1.error === 'INVALID_TOKEN') {
      console.log('✓ Successfully reproduced INVALID_TOKEN error (no JWT)');
    } else {
      console.log('✗ Unexpected error:', error1);
    }
  } catch (err) {
    console.error('Request failed:', err);
  }

  console.log('');

  // ========================================================================
  // TEST 2: Reproduce INVALID_TOKEN error (empty JWT)
  // ========================================================================
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('TEST 2: Call /account with EMPTY JWT (should fail)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('');

  try {
    const response2 = await fetch(`${BASE_URL}/account`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
        'Authorization': 'Bearer ', // Empty token - this is the bug!
      },
    });

    console.log('Response status:', response2.status, response2.statusText);
    const error2 = await response2.json();
    console.log('Error response:', JSON.stringify(error2, null, 2));
    console.log('');

    if (error2.error === 'INVALID_TOKEN' && error2.message?.includes('missing value')) {
      console.log('✓ Successfully reproduced "invalid bearer jwt: missing value" error');
      console.log('');
      console.log('THIS IS THE EXACT ERROR FROM THE BUG REPORT:');
      console.log('  INVALID_TOKEN: invalid bearer jwt: missing value in request header');
    } else {
      console.log('✗ Got different error than expected');
    }
  } catch (err) {
    console.error('Request failed:', err);
  }

  console.log('');

  // ========================================================================
  // TEST 3: Successful authentication with valid credentials
  // ========================================================================
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('TEST 3: Generate JWT and call /account (should succeed)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('');

  try {
    // Import Paradex SDK for config
    console.log('Loading Paradex SDK config...');
    const SDK = await import('@tealstreet/paradex-sdk');
    const environment = testCreds!.testnet ? 'testnet' : 'prod';
    const config = await SDK.Config.fetchConfig(environment);
    console.log('  - Chain ID:', config.paradexChainId);
    console.log('  - RPC URL:', config.paradexFullNodeRpcUrl);
    console.log('');

    // Create Starknet account with subkey
    console.log('Creating Starknet account...');
    const account = new Account(
      { nodeUrl: config.paradexFullNodeRpcUrl },
      testCreds!.paradexAddress, // Main account address
      testCreds!.apiKey // Subkey private key
    );
    console.log('  - Account address:', account.address);
    console.log('');

    // Generate JWT token
    console.log('Generating JWT token...');
    const timestamp = Math.floor(Date.now() / 1000);
    const expiry = timestamp + 24 * 60 * 60; // 24 hours

    const signature = await account.signMessage({
      domain: {
        name: 'Paradex',
        chainId: config.paradexChainId,
        version: '1',
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'chainId', type: 'felt' },
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
        method: 'POST',
        path: '/v1/auth',
        body: '',
        timestamp: timestamp,
        expiration: expiry,
      },
    });

    const sigArray = Array.isArray(signature)
      ? signature.map((s: any) => (typeof s === 'bigint' ? s.toString() : String(s)))
      : [String((signature as any).r), String((signature as any).s)];

    console.log('  - Signature:', sigArray);
    console.log('');

    // Call POST /auth/{subkey} to get JWT token
    console.log('Calling POST /auth/{subkey}...');
    const authEndpoint = `${BASE_URL}/auth/${testCreds!.subkeyAddress}`;
    const authResponse = await fetch(authEndpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
        'PARADEX-STARKNET-ACCOUNT': testCreds!.paradexAddress,
        'PARADEX-STARKNET-SIGNATURE': JSON.stringify(sigArray),
        'PARADEX-TIMESTAMP': timestamp.toString(),
        'PARADEX-SIGNATURE-EXPIRATION': expiry.toString(),
      },
    });

    console.log('  - Response status:', authResponse.status, authResponse.statusText);

    if (!authResponse.ok) {
      const errorText = await authResponse.text();
      console.error('  - Auth error:', errorText);
      console.log('');
      console.log('❌ JWT generation failed!');
      console.log('   This explains the INVALID_TOKEN error in validateAccount.');
      return;
    }

    const authData = await authResponse.json();
    const jwtToken = authData.jwt_token || authData.token;

    if (!jwtToken) {
      console.error('  - No JWT token in response:', authData);
      console.log('');
      console.log('❌ No JWT token returned!');
      console.log('   This explains the empty token error.');
      return;
    }

    console.log('  - JWT token:', jwtToken.substring(0, 50) + '...');
    console.log('');

    // Now call /account with the JWT token (this is what validateAccount does)
    console.log('Calling GET /account with JWT...');
    const accountResponse = await fetch(`${BASE_URL}/account`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json',
        'Authorization': `Bearer ${jwtToken}`,
      },
    });

    console.log('  - Response status:', accountResponse.status, accountResponse.statusText);

    if (!accountResponse.ok) {
      const errorText = await accountResponse.text();
      console.error('  - Account error:', errorText);
      console.log('');
      console.log('❌ Account call failed even with valid JWT!');
      return;
    }

    const accountData = await accountResponse.json();
    console.log('  - Account data:', JSON.stringify(accountData, null, 2));
    console.log('');
    console.log('✓ Successfully authenticated and called /account!');

  } catch (err) {
    console.error('Error:', err);
  }

  console.log('');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('SUMMARY');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('');
  console.log('The INVALID_TOKEN error occurs when:');
  console.log('  1. No Authorization header is sent');
  console.log('  2. Empty Bearer token is sent ("Bearer ")');
  console.log('  3. JWT generation fails but empty string is used anyway');
  console.log('');
  console.log('Root causes in validateAccount flow:');
  console.log('  1. jwtManager is null (SDK not initialized)');
  console.log('  2. jwtManager.getToken() returns empty string');
  console.log('  3. Token generation fails but error is swallowed');
  console.log('');
}

main().catch(console.error);
