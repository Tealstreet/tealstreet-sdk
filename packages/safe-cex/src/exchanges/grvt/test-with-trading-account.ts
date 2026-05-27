#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import axios from 'axios';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testWithTradingAccount() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Test with Trading Account ID ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('Trading Account ID:', config.grvtTradingAccountId || '(not set)');
  console.log('Signer Address:', config.grvtSignerAddress);
  console.log('=========================================\n');

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

    const setCookie = authResponse.headers['set-cookie'];
    if (setCookie) {
      const gravityCookie = setCookie.find((c: string) => c.startsWith('gravity='));
      if (gravityCookie) {
        sessionCookie = gravityCookie.split(';')[0].split('=')[1];
        console.log('✅ Session cookie:', sessionCookie?.substring(0, 20) + '...');
      }
    }

    accountId = authResponse.headers['x-grvt-account-id'];
    console.log('✅ Account ID:', accountId);
    console.log('\n');

    if (!sessionCookie || !accountId) {
      console.error('❌ Missing session cookie or account ID');
      return;
    }

    // Step 2: Try with trading account ID as sub_account_id
    if (config.grvtTradingAccountId) {
      console.log(`2. Testing account_summary with trading_account_id as sub_account_id: ${config.grvtTradingAccountId}...`);
      try {
        const balanceResponse = await axios.post(
          `${tradesUrl}/lite/v1/account_summary`,
          {
            sub_account_id: config.grvtTradingAccountId,
          },
          {
            headers: {
              'Content-Type': 'application/json',
              Cookie: `gravity=${sessionCookie}`,
              'X-Grvt-Account-Id': accountId,
            },
          }
        );

        console.log('✅ Success with trading_account_id! Response:');
        console.log(JSON.stringify(balanceResponse.data, null, 2));
      } catch (err: any) {
        console.error('❌ Failed:');
        console.error('  Status:', err.response?.status);
        console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
      }
      console.log('\n');
    }

    // Step 3: Try passing trading_account_id in different ways
    if (config.grvtTradingAccountId) {
      console.log(`3. Testing with trading_account_id as a separate field...`);
      try {
        const balanceResponse = await axios.post(
          `${tradesUrl}/lite/v1/account_summary`,
          {
            sub_account_id: '',
            trading_account_id: config.grvtTradingAccountId,
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
        console.error('❌ Failed:');
        console.error('  Status:', err.response?.status);
        console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
      }
      console.log('\n');
    }

    // Step 4: Try with trading_account_id in header
    if (config.grvtTradingAccountId) {
      console.log(`4. Testing with trading_account_id in header...`);
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
              'X-Grvt-Trading-Account-Id': config.grvtTradingAccountId,
            },
          }
        );

        console.log('✅ Success! Response:');
        console.log(JSON.stringify(balanceResponse.data, null, 2));
      } catch (err: any) {
        console.error('❌ Failed:');
        console.error('  Status:', err.response?.status);
        console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
      }
      console.log('\n');
    }

    // Step 5: Try replacing account ID with trading account ID
    if (config.grvtTradingAccountId) {
      console.log(`5. Testing with trading_account_id replacing account_id...`);
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
              'X-Grvt-Account-Id': config.grvtTradingAccountId,
            },
          }
        );

        console.log('✅ Success! Response:');
        console.log(JSON.stringify(balanceResponse.data, null, 2));
      } catch (err: any) {
        console.error('❌ Failed:');
        console.error('  Status:', err.response?.status);
        console.error('  Error:', JSON.stringify(err.response?.data, null, 2));
      }
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

testWithTradingAccount().catch(console.error);
