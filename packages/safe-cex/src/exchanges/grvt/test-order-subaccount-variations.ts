/**
 * Test order placement with different sub_account_id values
 *
 * Run with: npx tsx src/exchanges/grvt/test-order-subaccount-variations.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import axios from 'axios';
import { Wallet } from 'ethers';
import { signTypedData, SignTypedDataVersion } from '@metamask/eth-sig-util';

const credPath = path.resolve(__dirname, '.apikey.json');

interface ApiKeyConfig {
  apiKey: string;
  apiSecret: string;
  testnet?: boolean;
}

// EIP-712 types for GRVT order signing
const EIP712_DOMAIN = {
  name: 'Grvt',
  version: '0',
  chainId: 325, // mainnet
};

const ORDER_TYPES = {
  EIP712Domain: [
    { name: 'name', type: 'string' },
    { name: 'version', type: 'string' },
    { name: 'chainId', type: 'uint256' },
  ],
  Order: [
    { name: 'subAccountID', type: 'uint64' },
    { name: 'isMarket', type: 'bool' },
    { name: 'timeInForce', type: 'uint8' },
    { name: 'postOnly', type: 'bool' },
    { name: 'reduceOnly', type: 'bool' },
    { name: 'legs', type: 'OrderLeg[]' },
    { name: 'nonce', type: 'uint32' },
    { name: 'expiration', type: 'int64' },
  ],
  OrderLeg: [
    { name: 'assetID', type: 'uint256' },
    { name: 'contractSize', type: 'uint64' },
    { name: 'limitPrice', type: 'uint64' },
    { name: 'isBuyingContract', type: 'bool' },
  ],
};

async function tryPlaceOrder(
  tradesUrl: string,
  headers: Record<string, string>,
  wallet: Wallet,
  subAccountId: string | number,
  description: string
) {
  console.log(`\n--- Trying: ${description} ---`);
  console.log(`  sub_account_id value: "${subAccountId}" (type: ${typeof subAccountId})`);

  const now = Date.now();
  const nonce = Math.floor(Math.random() * 1000000);
  const expiration = (now + 3600000) * 1000000; // 1 hour from now in nanoseconds

  // BTC_USDT_Perp instrument hash
  const instrumentHash = '0x030501';
  const baseDecimals = 9;
  const PRICE_MULTIPLIER = 1_000_000_000;

  const size = 0.001;
  const price = 50000;
  const contractSize = Math.floor(size * Math.pow(10, baseDecimals));
  const limitPrice = Math.floor(price * PRICE_MULTIPLIER);

  // Parse subAccountId as integer for signing
  const subAccountIdInt = typeof subAccountId === 'string' ? parseInt(subAccountId, 10) || 0 : subAccountId;

  const eip712Message = {
    subAccountID: subAccountIdInt,
    isMarket: false,
    timeInForce: 1, // GOOD_TILL_TIME
    postOnly: false,
    reduceOnly: false,
    legs: [
      {
        assetID: instrumentHash,
        contractSize: contractSize,
        limitPrice: limitPrice,
        isBuyingContract: true,
      },
    ],
    nonce: nonce,
    expiration: expiration,
  };

  console.log(`  EIP-712 subAccountID (int): ${subAccountIdInt}`);

  // Sign the order
  const privateKeyBuffer = Buffer.from(wallet.privateKey.slice(2), 'hex');
  const signature = signTypedData({
    privateKey: privateKeyBuffer,
    data: {
      types: ORDER_TYPES,
      primaryType: 'Order',
      domain: EIP712_DOMAIN,
      message: eip712Message,
    },
    version: SignTypedDataVersion.V4,
  });

  const r = '0x' + signature.slice(2, 66);
  const s = '0x' + signature.slice(66, 130);
  const v = parseInt(signature.slice(130, 132), 16);

  // Build order in lite format - wrapped in 'o' object per API docs
  const liteOrder = {
    o: {
      sa: String(subAccountId), // sub_account_id as string
      im: false,
      ti: 'GOOD_TILL_TIME',
      po: false,
      ro: false,
      l: [
        {
          i: 'BTC_USDT_Perp',
          s: '0.001',
          lp: '50000',
          ib: true,
        },
      ],
      s: {
        s: wallet.address,
        r: r,
        s1: s,
        v: v,
        e: String(expiration),
        n: nonce,
        ci: '325',
      },
      m: {
        co: String(nonce),
        ct: String(now * 1000000),
      },
    },
  };

  console.log(`  Lite order sa: "${liteOrder.o.sa}"`);

  try {
    const response = await axios.post(`${tradesUrl}/lite/v1/create_order`, liteOrder, { headers });
    console.log('  ✅ SUCCESS! Response:', JSON.stringify(response.data, null, 2));
    return true;
  } catch (err: any) {
    const errData = err.response?.data;
    console.log(`  ❌ FAILED: ${err.response?.status} - ${errData?.m || errData?.message || err.message}`);
    if (errData) {
      console.log('  Error details:', JSON.stringify(errData, null, 2));
    }
    return false;
  }
}

async function main() {
  console.log('=== GRVT Order Sub-Account Variations Test ===\n');

  if (!fs.existsSync(credPath)) {
    console.log('⚠️ No credentials file found');
    process.exit(0);
  }

  const config: ApiKeyConfig = JSON.parse(fs.readFileSync(credPath, 'utf-8'));
  const isTestnet = config.testnet ?? false;
  const edgeUrl = isTestnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = isTestnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  // Initialize wallet
  const privateKey = config.apiSecret.startsWith('0x') ? config.apiSecret : `0x${config.apiSecret}`;
  const wallet = new Wallet(privateKey);
  console.log('Wallet address:', wallet.address);

  // Authenticate
  console.log('\nAuthenticating...');
  const authResponse = await axios.post(
    `${edgeUrl}/auth/api_key/login`,
    { api_key: config.apiKey },
    { headers: { 'Content-Type': 'application/json' } }
  );

  const cookies = authResponse.headers['set-cookie'];
  const accountIdHeader = authResponse.headers['x-grvt-account-id'];

  console.log('x-grvt-account-id:', accountIdHeader);

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

  // Get main account ID from aggregated summary
  console.log('\nFetching account info...');
  let mainAccountId = '';
  try {
    const aggResp = await axios.post(`${tradesUrl}/lite/v1/aggregated_account_summary`, {}, { headers });
    mainAccountId = aggResp.data?.r?.ma || '';
    console.log('main_account_id:', mainAccountId);
  } catch (err: any) {
    console.log('Could not fetch aggregated summary:', err.response?.data || err.message);
  }

  // Try different sub_account_id values
  const variations = [
    { value: '0', desc: 'sub_account_id = "0" (string zero)' },
    { value: 0, desc: 'sub_account_id = 0 (integer zero)' },
    { value: '', desc: 'sub_account_id = "" (empty string)' },
    { value: accountIdHeader, desc: `sub_account_id = x-grvt-account-id header (${accountIdHeader})` },
    { value: mainAccountId, desc: `sub_account_id = main_account_id (${mainAccountId})` },
  ];

  // Also try without the sa field entirely by modifying the request
  console.log('\n=== Testing variations ===');

  for (const variation of variations) {
    if (variation.value === undefined || variation.value === null) continue;
    const success = await tryPlaceOrder(tradesUrl, headers, wallet, variation.value, variation.desc);
    if (success) {
      console.log('\n🎉 Found working sub_account_id value!');
      break;
    }
  }

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
