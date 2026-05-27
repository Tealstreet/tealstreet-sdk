/**
 * Debug script to diagnose "Checking authorization status..." hanging issue.
 *
 * The GrvtBuilderFeeApproval component calls checkGrvtBuilderAuthorized()
 * which routes through the Tealstreet proxy. This script bypasses the proxy
 * and hits GRVT testnet directly to isolate where the hang occurs.
 *
 * Usage: npx tsx src/exchanges/grvt/test-builder-auth-check.ts
 */

import { existsSync, readFileSync } from 'fs';
import { resolve } from 'path';

import axios from 'axios';

const GRVT_BUILDER_ADDRESS = '0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca';

async function main() {
  const credPath = resolve(__dirname, '.apikey.test.json');

  if (!existsSync(credPath)) {
    console.error('❌ No .apikey.test.json found at', credPath);
    process.exit(1);
  }

  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('=== GRVT Builder Authorization Check Debug ===\n');
  console.log('API Key:', config.apiKey?.substring(0, 10) + '...');
  console.log('Testnet:', config.testnet);
  console.log('Trading Account ID:', config.grvtTradingAccountId);
  console.log('Builder to check:', GRVT_BUILDER_ADDRESS);

  // ============================================================
  // Test 1: Direct GRVT Authentication (no proxy)
  // ============================================================
  console.log('\n--- Test 1: Direct GRVT Authentication ---');
  const edgeUrl = 'https://edge.testnet.grvt.io';
  const tradesUrl = 'https://trades.testnet.grvt.io';

  let sessionCookie: string | undefined;
  let accountId: string | undefined;

  try {
    const authStart = Date.now();
    const authResponse = await axios.post(
      `${edgeUrl}/auth/api_key/login`,
      { api_key: config.apiKey },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: 'rm=true;',
        },
        timeout: 15000,
      },
    );
    console.log(`✅ Auth succeeded in ${Date.now() - authStart}ms (status: ${authResponse.status})`);

    const setCookie = authResponse.headers['set-cookie'];
    const gravityCookie = setCookie?.find((c: string) => c.startsWith('gravity='));
    sessionCookie = gravityCookie?.split(';')[0].split('=')[1];
    accountId = authResponse.headers['x-grvt-account-id'];

    console.log('   Session cookie:', sessionCookie ? sessionCookie.substring(0, 20) + '...' : 'MISSING');
    console.log('   Account ID:', accountId || 'MISSING');

    if (!sessionCookie || !accountId) {
      console.error('❌ Missing session cookie or account ID');
      process.exit(1);
    }
  } catch (err: any) {
    console.error('❌ Auth failed:', err.response?.data || err.message);
    process.exit(1);
  }

  // ============================================================
  // Test 2: Direct builder check (full format)
  // ============================================================
  console.log('\n--- Test 2: Direct get_authorized_builders (full format) ---');
  const subAccountId = config.grvtTradingAccountId || '0';

  try {
    const checkStart = Date.now();
    const buildersResponse = await axios.post(
      `${tradesUrl}/full/v1/get_authorized_builders`,
      { sub_account_id: subAccountId },
      {
        headers: {
          'Content-Type': 'application/json',
          Cookie: `gravity=${sessionCookie}`,
          'X-Grvt-Account-Id': accountId,
        },
        timeout: 15000,
      },
    );
    console.log(`✅ Builder check completed in ${Date.now() - checkStart}ms`);
    console.log('   Response:', JSON.stringify(buildersResponse.data, null, 2));

    const data = buildersResponse.data;
    const builders = data.results || data.r || [];
    console.log(`   Found ${builders.length} authorized builder(s)`);

    let found = false;
    for (const builderInfo of builders) {
      const addr = (builderInfo.builder_address || builderInfo.ba || '').toLowerCase();
      console.log(`   - Builder: ${addr}`);
      if (addr === GRVT_BUILDER_ADDRESS.toLowerCase()) {
        found = true;
        const maxFeeRate = builderInfo.max_fee_rate || builderInfo.mfr;
        console.log(`     ✅ Tealstreet builder found! Max fee rate: ${maxFeeRate}`);
      }
    }

    if (!found) {
      console.log('   ❌ Tealstreet builder NOT authorized for this account');
    }
  } catch (err: any) {
    console.error('❌ Builder check failed:', err.response?.status, err.response?.data || err.message);
  }

  // ============================================================
  // Test 3: Try via proxy with empty token (simulates the UI bug)
  // ============================================================
  console.log('\n--- Test 3: Proxy auth with empty token (simulates UI bug) ---');
  const GRVT_PROXY_URL = 'https://fsjnpy143pbz-tiny.jp.tealstreet.io/api/exchange_proxy';
  const proxyEdgeUrl = `${GRVT_PROXY_URL}/https://edge.testnet.grvt.io`;

  try {
    const proxyStart = Date.now();
    const proxyAuthResponse = await axios.post(
      `${proxyEdgeUrl}/auth/api_key/login`,
      { api_key: config.apiKey },
      {
        headers: {
          'Content-Type': 'application/json',
          'Tealstreet-Authorization': 'Bearer ', // Empty token
          'X-Grvt-Remember-Me': 'true',
        },
        timeout: 10000,
      },
    );
    console.log(`✅ Proxy auth succeeded in ${Date.now() - proxyStart}ms (unexpected!)`);
    console.log('   Status:', proxyAuthResponse.status);
  } catch (err: any) {
    if (err.code === 'ECONNABORTED' || err.message?.includes('timeout')) {
      console.log(`⏰ TIMEOUT after 10s — this is the hang!`);
      console.log('   The proxy drops requests with empty/invalid Bearer token');
    } else if (err.response) {
      console.log(`❌ Proxy returned ${err.response.status}:`, JSON.stringify(err.response.data));
    } else {
      console.log('❌ Proxy error:', err.message);
    }
  }

  // ============================================================
  // Test 4: Try via proxy using fetch() (matches UI code path)
  // ============================================================
  console.log('\n--- Test 4: Proxy auth with fetch() and empty token ---');

  try {
    const fetchStart = Date.now();
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 10000);

    const fetchResponse = await fetch(`${proxyEdgeUrl}/auth/api_key/login`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Tealstreet-Authorization': 'Bearer ',
        'X-Grvt-Remember-Me': 'true',
      },
      credentials: 'include',
      body: JSON.stringify({ api_key: config.apiKey }),
      signal: controller.signal,
    });
    clearTimeout(timeoutId);

    console.log(`Response in ${Date.now() - fetchStart}ms: status ${fetchResponse.status}`);
    const text = await fetchResponse.text();
    console.log('   Body:', text.substring(0, 200));
  } catch (err: any) {
    if (err.name === 'AbortError') {
      console.log('⏰ TIMEOUT after 10s — fetch() also hangs with empty token');
    } else {
      console.log('❌ Fetch error:', err.message);
    }
  }

  // ============================================================
  // Summary
  // ============================================================
  console.log('\n=== Summary ===');
  console.log('Direct API calls work fine. The issue is in checkGrvtBuilderAuthorized()');
  console.log('which calls getProxyToken() → returns empty string → proxy hangs/rejects.');
  console.log('\nFix options:');
  console.log('1. checkGrvtBuilderAuthorized should bypass proxy when called from settings UI');
  console.log('2. The GrvtBuilderFeeApproval component should use direct GRVT API calls');
  console.log('3. Add a timeout to the fetch() call in checkGrvtBuilderAuthorized');
}

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
