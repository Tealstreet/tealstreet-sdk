/**
 * Test to check API key info and permissions
 *
 * Run with: npx tsx src/exchanges/grvt/test-api-key-info.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import axios from 'axios';
import { Wallet } from 'ethers';

const credPath = path.resolve(__dirname, '.apikey.json');

async function main() {
  console.log('=== GRVT API Key Info Test ===\n');

  const config = JSON.parse(fs.readFileSync(credPath, 'utf-8'));
  const isTestnet = config.testnet ?? false;
  const edgeUrl = isTestnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = isTestnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  const privateKey = config.apiSecret.startsWith('0x') ? config.apiSecret : `0x${config.apiSecret}`;
  const wallet = new Wallet(privateKey);

  console.log('Configuration:');
  console.log('  API Key:', config.apiKey);
  console.log('  Private Key Signer:', wallet.address);
  console.log('  Testnet:', isTestnet);
  console.log('  Edge URL:', edgeUrl);
  console.log('  Trades URL:', tradesUrl);

  // Authenticate
  console.log('\n--- Authentication ---');
  const authResponse = await axios.post(
    `${edgeUrl}/auth/api_key/login`,
    { api_key: config.apiKey },
    { headers: { 'Content-Type': 'application/json' } }
  );

  console.log('Auth Response Status:', authResponse.status);
  console.log('Auth Response Headers:');
  console.log('  x-grvt-account-id:', authResponse.headers['x-grvt-account-id']);
  console.log('  set-cookie:', authResponse.headers['set-cookie']?.length, 'cookies');
  console.log('Auth Response Data:', JSON.stringify(authResponse.data, null, 2));

  const cookies = authResponse.headers['set-cookie'];
  const accountIdHeader = authResponse.headers['x-grvt-account-id'];

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

  // Try to get account info
  console.log('\n--- Account Info Endpoints ---');

  // Try sub_account_id = accountIdHeader
  console.log(`\nTrying account_summary with sa="${accountIdHeader}"...`);
  try {
    const resp = await axios.post(`${tradesUrl}/lite/v1/account_summary`, { sa: accountIdHeader }, { headers });
    console.log('✅ SUCCESS:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.log('❌ FAILED:', err.response?.status, err.response?.data?.m || err.message);
  }

  // Try sub_account_id = wallet address
  console.log(`\nTrying account_summary with sa="${wallet.address}"...`);
  try {
    const resp = await axios.post(`${tradesUrl}/lite/v1/account_summary`, { sa: wallet.address }, { headers });
    console.log('✅ SUCCESS:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.log('❌ FAILED:', err.response?.status, err.response?.data?.m || err.message);
  }

  // Get main account id from aggregated summary
  console.log('\nFetching aggregated_account_summary...');
  let mainAccountId = '';
  try {
    const resp = await axios.post(`${tradesUrl}/lite/v1/aggregated_account_summary`, {}, { headers });
    console.log('✅ SUCCESS:', JSON.stringify(resp.data, null, 2));
    mainAccountId = resp.data?.r?.ma || '';
    console.log('\nExtracted main_account_id:', mainAccountId);
  } catch (err: any) {
    console.log('❌ FAILED:', err.response?.status, err.response?.data?.m || err.message);
  }

  // Try sub_account_id = main_account_id
  if (mainAccountId) {
    console.log(`\nTrying account_summary with sa="${mainAccountId}"...`);
    try {
      const resp = await axios.post(`${tradesUrl}/lite/v1/account_summary`, { sa: mainAccountId }, { headers });
      console.log('✅ SUCCESS:', JSON.stringify(resp.data, null, 2));
    } catch (err: any) {
      console.log('❌ FAILED:', err.response?.status, err.response?.data?.m || err.message);
    }
  }

  // Try to list API keys or get key info
  console.log('\n--- Trying to get API key info ---');

  // Check if there's an endpoint to list tagged signers or API key permissions
  const endpointsToTry = [
    '/lite/v1/api_key_info',
    '/full/v1/api_key_info',
    '/lite/v1/get_api_key',
    '/lite/v1/list_api_keys',
    '/lite/v1/sub_accounts',
    '/lite/v1/list_sub_accounts',
    '/full/v1/sub_accounts',
  ];

  for (const endpoint of endpointsToTry) {
    console.log(`\nTrying ${endpoint}...`);
    try {
      const resp = await axios.post(`${tradesUrl}${endpoint}`, {}, { headers });
      console.log('✅ SUCCESS:', JSON.stringify(resp.data, null, 2));
    } catch (err: any) {
      console.log('❌ FAILED:', err.response?.status, err.response?.data?.m || err.response?.data?.message || err.message);
    }
  }

  console.log('\n=== Summary ===');
  console.log('x-grvt-account-id from auth:', accountIdHeader);
  console.log('main_account_id from aggregated_summary:', mainAccountId);
  console.log('Signer address from private key:', wallet.address);
  console.log('\nNote: If signer address != main_account_id, the signer might need to be tagged in GRVT UI');

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
