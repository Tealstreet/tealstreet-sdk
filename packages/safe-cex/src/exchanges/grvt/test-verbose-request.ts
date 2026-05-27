#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import axios from 'axios';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testVerbose() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Verbose Request Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('=================================\n');

  const edgeUrl = config.testnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = config.testnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  try {
    // Step 1: Authenticate
    console.log('1. AUTHENTICATION REQUEST:');
    console.log('   URL:', `${edgeUrl}/auth/api_key/login`);
    console.log('   Method: POST');
    console.log('   Body:', JSON.stringify({ api_key: config.apiKey }, null, 2));
    console.log('   Headers:', JSON.stringify({
      'Content-Type': 'application/json',
      Cookie: 'rm=true;',
    }, null, 2));
    console.log('\n   Sending request...\n');

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

    console.log('   ✅ Response Status:', authResponse.status);
    console.log('   Response Headers:');
    console.log('     set-cookie:', authResponse.headers['set-cookie']);
    console.log('     x-grvt-account-id:', authResponse.headers['x-grvt-account-id']);
    console.log('\n');

    // Extract session cookie
    const setCookie = authResponse.headers['set-cookie'];
    let sessionCookie: string | undefined;
    if (setCookie) {
      const gravityCookie = setCookie.find((c: string) => c.startsWith('gravity='));
      if (gravityCookie) {
        sessionCookie = gravityCookie.split(';')[0].split('=')[1];
      }
    }

    // Extract account ID
    const accountId = authResponse.headers['x-grvt-account-id'];

    if (!sessionCookie || !accountId) {
      console.error('❌ Missing session cookie or account ID');
      return;
    }

    console.log('   Extracted Values:');
    console.log('     Session Cookie:', sessionCookie.substring(0, 20) + '...');
    console.log('     Account ID:', accountId);
    console.log('\n');

    // Step 2: Balance request
    console.log('2. BALANCE REQUEST:');
    console.log('   URL:', `${tradesUrl}/lite/v1/account_summary`);
    console.log('   Method: POST');
    const requestBody = { sub_account_id: '' };
    console.log('   Body:', JSON.stringify(requestBody, null, 2));
    const requestHeaders = {
      'Content-Type': 'application/json',
      'Cookie': `gravity=${sessionCookie}`,
      'X-Grvt-Account-Id': accountId,
    };
    console.log('   Headers:', JSON.stringify(requestHeaders, null, 2));
    console.log('\n   Sending request...\n');

    try {
      const balanceResponse = await axios.post(
        `${tradesUrl}/lite/v1/account_summary`,
        requestBody,
        {
          headers: requestHeaders,
        }
      );

      console.log('   ✅ SUCCESS!');
      console.log('   Response Status:', balanceResponse.status);
      console.log('   Response Data:');
      console.log(JSON.stringify(balanceResponse.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ FAILED!');
      console.log('   Response Status:', err.response?.status);
      console.log('   Response Data:');
      console.log(JSON.stringify(err.response?.data, null, 2));
      console.log('\n   Error Details:');
      console.log('     Message:', err.message);
      if (err.response?.headers) {
        console.log('     Response Headers:', JSON.stringify(err.response.headers, null, 2));
      }
    }

  } catch (err: any) {
    console.error('\n❌ Unexpected Error:', err.message);
    if (err.response) {
      console.error('   Status:', err.response.status);
      console.error('   Data:', JSON.stringify(err.response.data, null, 2));
    }
  }

  console.log('\n=== Test Complete ===\n');
}

testVerbose().catch(console.error);
