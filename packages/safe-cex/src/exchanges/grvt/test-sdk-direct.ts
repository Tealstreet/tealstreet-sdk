#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import axios from 'axios';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testSDKDirect() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT SDK Direct Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('SubAccount ID:', config.subaccountId || '(default)');
  console.log('============================\n');

  const edgeUrl = config.testnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = config.testnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  let sessionCookie: string | undefined;
  let accountId: string | undefined;

  try {
    // Step 1: Authenticate
    console.log('1. Authenticating with GRVT...');
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

    console.log('\n');

    // Step 2: Try account_summary with empty sub_account_id (using SDK endpoints)
    console.log('2. Testing /lite/v1/account_summary with empty sub_account_id...');
    try {
      const balanceResponse = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        {
          sub_account_id: '',
        },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );

      console.log('✅ Success! Response:');
      console.log(JSON.stringify(balanceResponse.data, null, 2));
    } catch (err: any) {
      console.error('❌ Failed with empty sub_account_id:');
      console.error('  Status:', err.response?.status);
      console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
    }

    console.log('\n');

    // Step 3: Try with specific sub_account_id
    if (config.subaccountId) {
      console.log(`3. Testing /lite/v1/account_summary with sub_account_id: ${config.subaccountId}...`);
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

        console.log('✅ Success! Response:');
        console.log(JSON.stringify(balanceResponse.data, null, 2));
      } catch (err: any) {
        console.error('❌ Failed with specific sub_account_id:');
        console.error('  Status:', err.response?.status);
        console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
      }
    }

    console.log('\n');

    // Step 4: Try fetching open orders (another trading endpoint)
    console.log('4. Testing /lite/v1/open_orders...');
    try {
      const ordersResponse = await axios.post(
        `${tradesUrl}/lite/v1/open_orders`,
        {
          sub_account_id: config.subaccountId || '',
        },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );

      console.log('✅ Success! Response:');
      console.log(JSON.stringify(ordersResponse.data, null, 2));
    } catch (err: any) {
      console.error('❌ Failed to fetch orders:');
      console.error('  Status:', err.response?.status);
      console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
    }

    console.log('\n');

    // Step 5: Try fetching positions
    console.log('5. Testing /lite/v1/positions...');
    try {
      const positionsResponse = await axios.post(
        `${tradesUrl}/lite/v1/positions`,
        {
          sub_account_id: config.subaccountId || '',
        },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );

      console.log('✅ Success! Response:');
      console.log(JSON.stringify(positionsResponse.data, null, 2));
    } catch (err: any) {
      console.error('❌ Failed to fetch positions:');
      console.error('  Status:', err.response?.status);
      console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
    }

  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response) {
      console.error('Status:', err.response.status);
      console.error('Data:', JSON.stringify(err.response.data, null, 2));
    }
  }

  console.log('\n=== Test Complete ===\n');
}

testSDKDirect().catch(console.error);
