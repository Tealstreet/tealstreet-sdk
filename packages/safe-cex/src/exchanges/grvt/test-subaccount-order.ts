/**
 * Test script to figure out sub-account ID for order placement
 *
 * Run with: npx tsx src/exchanges/grvt/test-subaccount-order.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import axios from 'axios';
import { Wallet } from 'ethers';

interface ApiKeyConfig {
  apiKey: string;
  apiSecret: string;
  testnet?: boolean;
  grvtTradingAccountId?: string;
}

async function main() {
  console.log('=== GRVT Sub-Account Order Test ===\n');

  const configPath = path.join(__dirname, '.apikey.json');
  const config: ApiKeyConfig = JSON.parse(fs.readFileSync(configPath, 'utf-8'));

  console.log('Config:');
  console.log('  - API Key:', config.apiKey);
  console.log('  - Testnet:', config.testnet ?? false);
  console.log('  - grvtTradingAccountId:', config.grvtTradingAccountId || '(not set)');

  const isTestnet = config.testnet ?? false;
  const edgeUrl = isTestnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = isTestnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  // Authenticate
  console.log('\n--- Step 1: Authentication ---');
  const authResponse = await axios.post(
    `${edgeUrl}/auth/api_key/login`,
    { api_key: config.apiKey },
    { headers: { 'Content-Type': 'application/json' } }
  );

  const cookies = authResponse.headers['set-cookie'];
  const accountIdHeader = authResponse.headers['x-grvt-account-id'];

  console.log('Auth successful!');
  console.log('  - x-grvt-account-id header:', accountIdHeader);

  let sessionCookie = '';
  if (cookies) {
    for (const cookie of cookies) {
      if (cookie.includes('gravity=')) {
        sessionCookie = cookie;
        break;
      }
    }
  }

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'Cookie': sessionCookie,
  };
  if (accountIdHeader) {
    headers['x-grvt-account-id'] = accountIdHeader;
  }

  // Try to get sub-accounts list
  console.log('\n--- Step 2: Try to find sub-accounts ---');

  // Try different endpoints that might return sub-account info
  const endpointsToTry = [
    '/lite/v1/aggregated_account_summary',
    '/full/v1/aggregated_account_summary',
  ];

  for (const endpoint of endpointsToTry) {
    console.log(`\nTrying ${endpoint}...`);
    try {
      const resp = await axios.post(`${tradesUrl}${endpoint}`, {}, { headers });
      console.log('Response:', JSON.stringify(resp.data, null, 2));

      // Look for any sub-account related fields
      const data = resp.data?.r || resp.data?.result;
      if (data) {
        console.log('\nLooking for sub-account fields:');
        console.log('  - sas (sub_account_summaries):', data.sas || data.sub_account_summaries || '(not present)');
        console.log('  - ma (main_account_id):', data.ma || data.main_account_id);

        // Check all keys
        console.log('\nAll keys in response:', Object.keys(data));
      }
    } catch (err: any) {
      console.error('Error:', err.response?.status, err.response?.data || err.message);
    }
  }

  // Try to get sub-account by using the account ID from header
  console.log('\n--- Step 3: Try account_summary with header account ID ---');
  try {
    // The x-grvt-account-id might be the sub-account ID in a different format
    const resp = await axios.post(
      `${tradesUrl}/lite/v1/account_summary`,
      { sub_account_id: accountIdHeader },
      { headers }
    );
    console.log('Response:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.status, err.response?.data || err.message);
  }

  // Try with numeric 0
  console.log('\n--- Step 4: Try account_summary with sub_account_id = "0" ---');
  try {
    const resp = await axios.post(
      `${tradesUrl}/lite/v1/account_summary`,
      { sub_account_id: '0' },
      { headers }
    );
    console.log('Response:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.status, err.response?.data || err.message);
  }

  // Try creating a wallet and getting the signer address
  console.log('\n--- Step 5: Derive signer address from private key ---');
  const privateKey = config.apiSecret.startsWith('0x') ? config.apiSecret : `0x${config.apiSecret}`;
  const wallet = new Wallet(privateKey);
  console.log('  - Signer address:', wallet.address);
  console.log('  - This address should be tagged to your API key in GRVT UI');

  // Check if maybe we need to use the main_account_id as sub_account_id
  console.log('\n--- Step 6: Try using main_account_id as sub_account_id ---');
  try {
    // Get main account ID first
    const aggResp = await axios.post(`${tradesUrl}/lite/v1/aggregated_account_summary`, {}, { headers });
    const mainAccountId = aggResp.data?.r?.ma || aggResp.data?.result?.main_account_id;
    console.log('  - main_account_id:', mainAccountId);

    if (mainAccountId) {
      // Try using it as sub_account_id
      const resp = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        { sub_account_id: mainAccountId },
        { headers }
      );
      console.log('Response:', JSON.stringify(resp.data, null, 2));
    }
  } catch (err: any) {
    console.error('Error:', err.response?.status, err.response?.data || err.message);
  }

  console.log('\n=== Test Complete ===');
  console.log('\nSummary:');
  console.log('  - x-grvt-account-id from auth:', accountIdHeader);
  console.log('  - Signer address:', wallet.address);
  console.log('\nTo place orders, you may need to set grvtTradingAccountId in your account config.');
  console.log('Try using one of the IDs discovered above.');
}

main().catch(console.error);
