/**
 * Debug script for GRVT authentication issues
 * Run with: npx tsx src/exchanges/grvt/test-auth-debug.ts
 *
 * IMPORTANT: This script tests GRVT authentication in CLI mode.
 * - Direct API mode: Tests GRVT auth without going through Tealstreet proxy
 * - The Tealstreet proxy requires a browser context (SharedWorker) for auth tokens
 *
 * Common error codes:
 * - 1000: Not authenticated (missing session cookie)
 * - 1001: Signer address not tagged to API key (go to GRVT UI and tag your address)
 * - 1400: No trade permission for signer
 * - 2000: Signature from unauthorized signer
 * - 2002: Signature doesn't match payload
 */

import axios from 'axios';
import { readFileSync } from 'fs';
import { join } from 'path';
import { Wallet } from 'ethers';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet: boolean;
  grvtSignerAddress?: string;
  grvtTradingAccountId?: string;
}

/**
 * Test GRVT authentication flow directly (without proxy)
 * This is the core test that verifies GRVT API key + session cookie authentication
 */
async function testDirectApiFlow(config: ApiKeyConfig) {
  console.log('\n=== Testing Direct GRVT API Authentication (No Proxy) ===\n');

  const edgeBaseUrl = config.testnet
    ? 'https://edge.testnet.grvt.io'
    : 'https://edge.grvt.io';

  const tradingBaseUrl = config.testnet
    ? 'https://trades.testnet.grvt.io'
    : 'https://trades.grvt.io';

  console.log(`Edge URL: ${edgeBaseUrl}`);
  console.log(`Trading URL: ${tradingBaseUrl}`);

  // Step 1: Authenticate with API key
  console.log('\n--- Step 1: API Key Authentication ---');
  let sessionCookie = '';
  let accountId = '';

  try {
    const authResponse = await axios.post(
      `${edgeBaseUrl}/auth/api_key/login`,
      { api_key: config.apiKey },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: 'rm=true;',
        },
        withCredentials: true,
      }
    );

    console.log('✅ Auth response status:', authResponse.status);
    console.log('   Response data:', JSON.stringify(authResponse.data, null, 2));

    // Extract session cookie
    const setCookie = authResponse.headers['set-cookie'];
    accountId = authResponse.headers['x-grvt-account-id'] || '';

    console.log('   X-Grvt-Account-Id:', accountId);

    if (setCookie) {
      const cookieArr = Array.isArray(setCookie) ? setCookie : [setCookie];
      const gravityCookie = cookieArr.find((c: string) => c.startsWith('gravity='));
      if (gravityCookie) {
        sessionCookie = gravityCookie.split(';')[0].split('=')[1];
        console.log('   Session cookie:', sessionCookie.substring(0, 30) + '...');
      }
    }

    if (!sessionCookie || !accountId) {
      console.log('❌ Missing session cookie or account ID');
      return;
    }
  } catch (err: any) {
    console.log('❌ Auth request failed:');
    console.log('   Status:', err.response?.status);
    console.log('   Data:', JSON.stringify(err.response?.data, null, 2));
    console.log('   Message:', err.message);
    return;
  }

  // Step 2: Test authenticated trading API calls
  console.log('\n--- Step 2: Test Trading API (Funding Account Summary) ---');
  try {
    const response = await axios.post(
      `${tradingBaseUrl}/lite/v1/funding_account_summary`,
      {},
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: `rm=true; gravity=${sessionCookie}`,
          'X-Grvt-Account-Id': accountId,
        },
      }
    );

    console.log('✅ Funding account summary SUCCESS');
    console.log('   Response:', JSON.stringify(response.data, null, 2).substring(0, 500));
  } catch (err: any) {
    console.log('❌ Funding account summary FAILED:');
    console.log('   Status:', err.response?.status);
    console.log('   Data:', JSON.stringify(err.response?.data, null, 2));
    handleGrvtError(err.response?.data);
  }

  // Step 3: Test sub-accounts listing
  console.log('\n--- Step 3: List Sub-accounts ---');
  try {
    const response = await axios.post(
      `${tradingBaseUrl}/lite/v1/sub_accounts`,
      {},
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: `rm=true; gravity=${sessionCookie}`,
          'X-Grvt-Account-Id': accountId,
        },
      }
    );

    console.log('✅ Sub-accounts response:');
    console.log('   Response:', JSON.stringify(response.data, null, 2));

    // If we got sub-accounts, try getting account summary for the first one
    const subAccounts = response.data?.r || response.data?.result || [];
    if (subAccounts.length > 0) {
      const firstSubAccount = subAccounts[0];
      console.log('\n--- Step 4: Get Account Summary for Sub-account ---');
      console.log('   Using sub_account_id:', firstSubAccount.sa || firstSubAccount.sub_account_id);

      try {
        const summaryResponse = await axios.post(
          `${tradingBaseUrl}/lite/v1/account_summary`,
          { sub_account_id: firstSubAccount.sa || firstSubAccount.sub_account_id },
          {
            headers: {
              'Content-Type': 'application/json',
              Cookie: `rm=true; gravity=${sessionCookie}`,
              'X-Grvt-Account-Id': accountId,
            },
          }
        );

        console.log('✅ Account summary SUCCESS');
        console.log('   Response:', JSON.stringify(summaryResponse.data, null, 2).substring(0, 500));
      } catch (err: any) {
        console.log('❌ Account summary FAILED:');
        console.log('   Status:', err.response?.status);
        console.log('   Data:', JSON.stringify(err.response?.data, null, 2));
        handleGrvtError(err.response?.data);
      }
    }
  } catch (err: any) {
    console.log('❌ Sub-accounts list FAILED:');
    console.log('   Status:', err.response?.status);
    console.log('   Data:', JSON.stringify(err.response?.data, null, 2));
    handleGrvtError(err.response?.data);
  }

  // Step 5: Test open orders (this often triggers error 1001 if signer not tagged)
  console.log('\n--- Step 5: Get Open Orders ---');
  try {
    const response = await axios.post(
      `${tradingBaseUrl}/lite/v1/open_orders`,
      { sub_account_id: config.grvtTradingAccountId || '' },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: `rm=true; gravity=${sessionCookie}`,
          'X-Grvt-Account-Id': accountId,
        },
      }
    );

    console.log('✅ Open orders SUCCESS');
    console.log('   Response:', JSON.stringify(response.data, null, 2).substring(0, 500));
  } catch (err: any) {
    console.log('❌ Open orders FAILED:');
    console.log('   Status:', err.response?.status);
    console.log('   Data:', JSON.stringify(err.response?.data, null, 2));
    handleGrvtError(err.response?.data);
  }
}

/**
 * Handle GRVT-specific error codes with helpful messages
 */
function handleGrvtError(data: any) {
  const errorCode = data?.c || data?.code;

  switch (errorCode) {
    case 1000:
      console.log('\n💡 ERROR 1000 - NOT AUTHENTICATED:');
      console.log('   Session cookie may be invalid or expired.');
      console.log('   Try re-authenticating with your API key.');
      break;
    case 1001:
      console.log('\n💡 ERROR 1001 - SIGNER NOT TAGGED:');
      console.log('   Your signer address is NOT tagged to this API key.');
      console.log('   Go to GRVT website → Account → API Keys');
      console.log('   Tag your Ethereum address to the API key.');
      break;
    case 1400:
      console.log('\n💡 ERROR 1400 - NO TRADE PERMISSION:');
      console.log('   The signer does not have trade permissions.');
      console.log('   Check API key permissions in GRVT UI.');
      break;
    case 2000:
      console.log('\n💡 ERROR 2000 - UNAUTHORIZED SIGNER:');
      console.log('   The signature is from an unauthorized signer.');
      console.log('   Make sure the correct private key is being used.');
      break;
    case 2002:
      console.log('\n💡 ERROR 2002 - SIGNATURE MISMATCH:');
      console.log('   The signature does not match the payload.');
      console.log('   Check EIP-712 signing implementation.');
      break;
  }
}

async function main() {
  // Load credentials
  const configPath = join(__dirname, '.apikey.json');
  let config: ApiKeyConfig;

  try {
    const configData = readFileSync(configPath, 'utf-8');
    config = JSON.parse(configData);
    console.log('✅ Loaded credentials from .apikey.json');
    console.log(`   API Key: ${config.apiKey.substring(0, 10)}...`);
    console.log(`   Testnet: ${config.testnet}`);
    console.log(`   Trading Account ID: ${config.grvtTradingAccountId || '(not set)'}`);

    // Verify signer address matches private key
    if (config.secret) {
      const wallet = new Wallet(config.secret);
      const derivedAddress = wallet.address;
      console.log(`   Derived Signer Address: ${derivedAddress}`);
      console.log(`   Configured Signer Address: ${config.grvtSignerAddress || 'not configured'}`);

      if (
        config.grvtSignerAddress &&
        derivedAddress.toLowerCase() !== config.grvtSignerAddress.toLowerCase()
      ) {
        console.log('⚠️ WARNING: Configured signer address does NOT match derived address!');
      }

      console.log('');
      console.log('📋 IMPORTANT: Make sure this address is TAGGED to your API key in GRVT UI:');
      console.log(`   ${derivedAddress}`);
      console.log(
        `   Go to: https://${config.testnet ? 'testnet.' : ''}grvt.io/exchange/account/api-keys`
      );
    }
  } catch (err) {
    console.error('❌ Failed to load .apikey.json:', err);
    console.log('');
    console.log('Create a .apikey.json file with the following format:');
    console.log(
      JSON.stringify(
        {
          apiKey: '<your-api-key>',
          secret: '<your-private-key-0x...>',
          testnet: true,
          grvtTradingAccountId: '<optional-sub-account-id>',
        },
        null,
        2
      )
    );
    return;
  }

  // Test direct API flow (no proxy)
  // This is the proper way to test GRVT auth in CLI since the proxy requires browser auth
  await testDirectApiFlow(config);

  console.log('\n');
  console.log('='.repeat(60));
  console.log('NOTES:');
  console.log('- This test uses direct API calls (no Tealstreet proxy)');
  console.log('- The Tealstreet proxy requires browser authentication');
  console.log('- For proxy testing, use the browser-based app');
  console.log('='.repeat(60));
}

main().catch(console.error);
