/**
 * Test script to explore all GRVT balance-related endpoints
 *
 * This tests:
 * 1. /lite/v1/funding_account_summary - Funding account balance
 * 2. /lite/v1/account_summary - Trading sub-account balance
 * 3. /lite/v1/aggregated_account_summary - All sub-accounts aggregated
 *
 * Run with: npx tsx src/exchanges/grvt/test-balance-endpoints.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import axios from 'axios';

interface ApiKeyConfig {
  apiKey: string;
  apiSecret: string;
  testnet?: boolean;
}

async function loadConfig(): Promise<ApiKeyConfig> {
  const configPath = path.join(__dirname, '.apikey.json');
  const configData = fs.readFileSync(configPath, 'utf-8');
  return JSON.parse(configData);
}

async function main() {
  console.log('=== GRVT Balance Endpoints Test ===\n');

  const config = await loadConfig();
  console.log('Config loaded:');
  console.log('  - API Key:', config.apiKey);
  console.log('  - Testnet:', config.testnet ?? false);

  const isTestnet = config.testnet ?? false;
  const edgeUrl = isTestnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = isTestnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  // First, authenticate
  console.log('\n--- Step 1: Authentication ---');
  console.log('  - Edge URL:', edgeUrl);
  console.log('  - Trades URL:', tradesUrl);

  const authResponse = await axios.post(
    `${edgeUrl}/auth/api_key/login`,
    { api_key: config.apiKey },
    { headers: { 'Content-Type': 'application/json' } }
  );

  const cookies = authResponse.headers['set-cookie'];
  const accountIdHeader = authResponse.headers['x-grvt-account-id'];

  console.log('Auth successful!');
  console.log('  - Account ID:', accountIdHeader);
  console.log('  - Has cookies:', !!cookies);

  // Extract session cookie
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

  console.log('  - Session Cookie:', sessionCookie.substring(0, 50) + '...');

  // Test 1: Funding Account Summary
  console.log('\n--- Test 1: /lite/v1/funding_account_summary ---');
  try {
    const fundingResp = await axios.post(`${tradesUrl}/lite/v1/funding_account_summary`, {}, { headers });
    console.log('Response:', JSON.stringify(fundingResp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.status, err.response?.data || err.message);
    if (err.response?.headers) {
      console.error('Headers:', JSON.stringify(err.response.headers, null, 2));
    }
  }

  // Test 2: Account Summary (with empty sub_account_id)
  console.log('\n--- Test 2: /lite/v1/account_summary (empty sub_account_id) ---');
  try {
    const accountResp = await axios.post(`${tradesUrl}/lite/v1/account_summary`, { sub_account_id: '' }, { headers });
    console.log('Response:', JSON.stringify(accountResp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.data || err.message);
  }

  // Test 3: Account Summary (with account_id as sub_account_id)
  if (accountIdHeader) {
    console.log(`\n--- Test 3: /lite/v1/account_summary (sub_account_id=${accountIdHeader}) ---`);
    try {
      const accountResp = await axios.post(`${tradesUrl}/lite/v1/account_summary`, { sub_account_id: accountIdHeader }, { headers });
      console.log('Response:', JSON.stringify(accountResp.data, null, 2));
    } catch (err: any) {
      console.error('Error:', err.response?.data || err.message);
    }
  }

  // Test 4: Aggregated Account Summary
  console.log('\n--- Test 4: /lite/v1/aggregated_account_summary ---');
  try {
    const aggResp = await axios.post(`${tradesUrl}/lite/v1/aggregated_account_summary`, {}, { headers });
    console.log('Response:', JSON.stringify(aggResp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.data || err.message);
  }

  // Test 5: Try using FULL endpoint for more verbose response
  console.log('\n--- Test 5: /full/v1/funding_account_summary ---');
  try {
    const fullFundingResp = await axios.post(`${tradesUrl}/full/v1/funding_account_summary`, {}, { headers });
    console.log('Response:', JSON.stringify(fullFundingResp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.data || err.message);
  }

  // Test 6: Full Account Summary
  console.log('\n--- Test 6: /full/v1/account_summary (empty sub_account_id) ---');
  try {
    const fullAccountResp = await axios.post(`${tradesUrl}/full/v1/account_summary`, { sub_account_id: '' }, { headers });
    console.log('Response:', JSON.stringify(fullAccountResp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.data || err.message);
  }

  // Test 7: Full Aggregated Account Summary
  console.log('\n--- Test 7: /full/v1/aggregated_account_summary ---');
  try {
    const fullAggResp = await axios.post(`${tradesUrl}/full/v1/aggregated_account_summary`, {}, { headers });
    console.log('Response:', JSON.stringify(fullAggResp.data, null, 2));
  } catch (err: any) {
    console.error('Error:', err.response?.data || err.message);
  }

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
