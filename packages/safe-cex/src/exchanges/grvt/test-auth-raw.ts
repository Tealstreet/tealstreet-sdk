#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import axios from 'axios';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(1);
}

const config = JSON.parse(readFileSync(credPath, 'utf-8'));

console.log('\n=== GRVT Auth Debug ===');
console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
console.log('Testnet:', config.testnet);
console.log('SubAccount ID:', config.subaccountId);
console.log('Signer Address:', config.grvtSignerAddress);

async function test() {
  // Step 1: Authenticate
  console.log('\n1. Authenticating...');
  const edgeUrl = config.testnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';

  let sessionCookie: string | undefined;
  let accountId: string | undefined;

  try {
    const authResponse = await axios.post(
      `${edgeUrl}/auth/api_key/login`,
      {
        api_key: config.apiKey,
      },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: 'rm=true;',
        },
        withCredentials: true,
      }
    );

    console.log('✅ Auth response status:', authResponse.status);

    // Extract session cookie
    const setCookie = authResponse.headers['set-cookie'];
    if (setCookie) {
      const gravityCookie = setCookie.find((c: string) => c.startsWith('gravity='));
      if (gravityCookie) {
        sessionCookie = gravityCookie.split(';')[0].split('=')[1];
        console.log('✅ Session cookie:', sessionCookie?.substring(0, 20) + '...');
      }
    }

    // Extract account ID
    accountId = authResponse.headers['x-grvt-account-id'];
    console.log('✅ Account ID:', accountId);

    if (!sessionCookie || !accountId) {
      console.error('❌ Missing session cookie or account ID');
      return;
    }

  } catch (err: any) {
    console.error('❌ Authentication failed:', err.response?.data || err.message);
    return;
  }

  // Step 2: Try fetching balance with empty sub-account ID (default)
  console.log('\n2. Fetching balance with empty sub-account ID...');
  const tradesUrl = config.testnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  try {
    const balanceResponse = await axios.post(
      `${tradesUrl}/lite/v1/account_summary`,
      {
        sub_account_id: '', // Empty string for default sub-account
      },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: `gravity=${sessionCookie}`,
          'X-Grvt-Account-Id': accountId,
        },
      }
    );

    console.log('✅ Balance fetch successful (empty sub-account ID)!');
    const summary = balanceResponse.data?.r;
    if (summary) {
      console.log('  Total Collateral:', summary.aggregate?.collateral || '0');
      console.log('  Account Value:', summary.aggregate?.account_value || '0');
    }

  } catch (err: any) {
    console.error('❌ Balance fetch failed (empty sub-account ID):');
    console.error('  Status:', err.response?.status);
    console.error('  Error:', err.response?.data?.error || err.message);
  }

  // Step 3: Try fetching balance with specific sub-account ID
  if (config.subaccountId) {
    console.log(`\n3. Fetching balance with sub-account ID: ${config.subaccountId}...`);

    try {
      const balanceResponse = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        {
          sub_account_id: config.subaccountId,
        },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );

      console.log('✅ Balance fetch successful (with sub-account ID)!');
      const summary = balanceResponse.data?.r;
      if (summary) {
        console.log('  Total Collateral:', summary.aggregate?.collateral || '0');
        console.log('  Account Value:', summary.aggregate?.account_value || '0');
      }

    } catch (err: any) {
      console.error('❌ Balance fetch failed (with sub-account ID):');
      console.error('  Status:', err.response?.status);
      console.error('  Error Code:', err.response?.data?.error?.code);
      console.error('  Error Message:', err.response?.data?.error?.message);
      console.error('  Full Error:', JSON.stringify(err.response?.data, null, 2));
    }
  }

  console.log('\n=== Debug Complete ===\n');
}

test().catch(console.error);
