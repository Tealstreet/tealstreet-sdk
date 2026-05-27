#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import axios from 'axios';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testVariations() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Request Variations Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('====================================\n');

  const edgeUrl = 'https://edge.grvt.io';
  const tradesUrl = 'https://trades.grvt.io';

  try {
    // Authenticate
    const authResponse = await axios.post(
      `${edgeUrl}/auth/api_key/login`,
      { api_key: config.apiKey },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: 'rm=true;',
        },
        withCredentials: true,
      }
    );

    const setCookie = authResponse.headers['set-cookie'];
    let sessionCookie: string | undefined;
    if (setCookie) {
      const gravityCookie = setCookie.find((c: string) => c.startsWith('gravity='));
      if (gravityCookie) {
        sessionCookie = gravityCookie.split(';')[0].split('=')[1];
      }
    }
    const accountId = authResponse.headers['x-grvt-account-id'];

    console.log('✅ Authenticated');
    console.log('   Session Cookie:', sessionCookie?.substring(0, 20) + '...');
    console.log('   Account ID:', accountId);
    console.log('\n');

    if (!sessionCookie || !accountId) {
      console.error('❌ Missing credentials');
      return;
    }

    // Test 1: Empty body
    console.log('1. Testing with empty body {}...');
    try {
      const res = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        {},
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Data:', JSON.stringify(res.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m);
    }
    console.log('\n');

    // Test 2: No body at all
    console.log('2. Testing with no body (undefined)...');
    try {
      const res = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        undefined,
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Data:', JSON.stringify(res.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m);
    }
    console.log('\n');

    // Test 3: Try the full format endpoint
    console.log('3. Testing /full/v1/account_summary...');
    try {
      const res = await axios.post(
        `${tradesUrl}/full/v1/account_summary`,
        { subAccountId: '' },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Data:', JSON.stringify(res.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m || err.response?.data?.error?.message);
    }
    console.log('\n');

    // Test 4: Try instruments endpoint (public)
    console.log('4. Testing public endpoint /lite/v1/all_instruments...');
    try {
      const res = await axios.post(
        `https://market-data.grvt.io/lite/v1/all_instruments`,
        {},
        {
          headers: {
            'Content-Type': 'application/json',
          },
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Instruments count:', res.data?.r?.length || 0);
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m);
    }
    console.log('\n');

    // Test 5: Try without the Cookie header (just account ID)
    console.log('5. Testing without Cookie header (only X-Grvt-Account-Id)...');
    try {
      const res = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        { sub_account_id: '' },
        {
          headers: {
            'Content-Type': 'application/json',
            'X-Grvt-Account-Id': accountId,
          },
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Data:', JSON.stringify(res.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m);
    }
    console.log('\n');

    // Test 6: Try without X-Grvt-Account-Id (just cookie)
    console.log('6. Testing without X-Grvt-Account-Id (only Cookie)...');
    try {
      const res = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        { sub_account_id: '' },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
          },
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Data:', JSON.stringify(res.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m);
    }
    console.log('\n');

    // Test 7: Try with withCredentials: true
    console.log('7. Testing with withCredentials: true...');
    try {
      const res = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        { sub_account_id: '' },
        {
          headers: {
            'Content-Type': 'application/json',
            Cookie: `gravity=${sessionCookie}`,
            'X-Grvt-Account-Id': accountId,
          },
          withCredentials: true,
        }
      );
      console.log('   ✅ SUCCESS:', res.status);
      console.log('   Data:', JSON.stringify(res.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ Failed:', err.response?.status, err.response?.data?.m);
    }

  } catch (err: any) {
    console.error('\n❌ Unexpected Error:', err.message);
  }

  console.log('\n=== Test Complete ===\n');
}

testVariations().catch(console.error);
