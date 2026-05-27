/**
 * Test order placement using FULL endpoint format
 *
 * Run with: npx tsx src/exchanges/grvt/test-order-full-endpoint.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import axios from 'axios';
import { Wallet } from 'ethers';
import { signTypedData, SignTypedDataVersion } from '@metamask/eth-sig-util';

const credPath = path.resolve(__dirname, '.apikey.json');

async function main() {
  console.log('=== GRVT Order Test - Full Endpoint ===\n');

  const config = JSON.parse(fs.readFileSync(credPath, 'utf-8'));
  const isTestnet = config.testnet ?? false;
  const edgeUrl = isTestnet ? 'https://edge.testnet.grvt.io' : 'https://edge.grvt.io';
  const tradesUrl = isTestnet ? 'https://trades.testnet.grvt.io' : 'https://trades.grvt.io';

  // Initialize wallet
  const privateKey = config.apiSecret.startsWith('0x') ? config.apiSecret : `0x${config.apiSecret}`;
  const wallet = new Wallet(privateKey);
  console.log('Wallet/Signer address:', wallet.address);

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

  // EIP-712 setup
  const EIP712_DOMAIN = { name: 'Grvt', version: '0', chainId: 325 };
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

  const now = Date.now();
  const nonce = Math.floor(Math.random() * 1000000);
  const expiration = (now + 3600000) * 1000000; // 1 hour in nanoseconds

  const instrumentHash = '0x030501';
  const baseDecimals = 9;
  const PRICE_MULTIPLIER = 1_000_000_000;
  const size = 0.001;
  const price = 50000;
  const contractSize = Math.floor(size * Math.pow(10, baseDecimals));
  const limitPrice = Math.floor(price * PRICE_MULTIPLIER);

  // Try with sub_account_id = "0"
  const subAccountId = '0';
  const subAccountIdInt = 0;

  const eip712Message = {
    subAccountID: subAccountIdInt,
    isMarket: false,
    timeInForce: 1,
    postOnly: false,
    reduceOnly: false,
    legs: [{
      assetID: instrumentHash,
      contractSize: contractSize,
      limitPrice: limitPrice,
      isBuyingContract: true,
    }],
    nonce: nonce,
    expiration: expiration,
  };

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

  // Build order in FULL format (not lite)
  const fullOrder = {
    order: {
      sub_account_id: subAccountId,
      is_market: false,
      time_in_force: 'GOOD_TILL_TIME',
      post_only: false,
      reduce_only: false,
      legs: [{
        instrument: 'BTC_USDT_Perp',
        size: '0.001',
        limit_price: '50000',
        is_buying_asset: true,
      }],
      signature: {
        signer: wallet.address,
        r: r,
        s: s,
        v: v,
        expiration: String(expiration),
        nonce: nonce,
        chain_id: '325',
      },
      metadata: {
        client_order_id: String(nonce),
        create_time: String(now * 1000000),
      },
    },
  };

  console.log('\nOrder details:');
  console.log('  sub_account_id:', subAccountId);
  console.log('  signer:', wallet.address);
  console.log('  instrument: BTC_USDT_Perp');
  console.log('  size: 0.001');
  console.log('  price: 50000');

  console.log('\n--- Trying FULL endpoint: /full/v1/create_order ---');
  try {
    const response = await axios.post(`${tradesUrl}/full/v1/create_order`, fullOrder, { headers });
    console.log('✅ SUCCESS! Response:', JSON.stringify(response.data, null, 2));
  } catch (err: any) {
    console.log('❌ FAILED:', err.response?.status);
    console.log('Error:', JSON.stringify(err.response?.data, null, 2));
  }

  console.log('\n--- Trying LITE endpoint: /lite/v1/create_order ---');
  // Also try lite format with proper wrapping
  const liteOrder = {
    o: {
      sa: subAccountId,
      im: false,
      ti: 'GOOD_TILL_TIME',
      po: false,
      ro: false,
      l: [{
        i: 'BTC_USDT_Perp',
        s: '0.001',
        lp: '50000',
        ib: true,
      }],
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

  try {
    const response = await axios.post(`${tradesUrl}/lite/v1/create_order`, liteOrder, { headers });
    console.log('✅ SUCCESS! Response:', JSON.stringify(response.data, null, 2));
  } catch (err: any) {
    console.log('❌ FAILED:', err.response?.status);
    console.log('Error:', JSON.stringify(err.response?.data, null, 2));
  }

  // Check API key permissions by trying other endpoints
  console.log('\n--- Checking API permissions ---');

  console.log('\nTrying /lite/v1/open_orders...');
  try {
    const resp = await axios.post(`${tradesUrl}/lite/v1/open_orders`, { sa: '0' }, { headers });
    console.log('✅ open_orders works:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.log('❌ open_orders failed:', err.response?.status, err.response?.data?.m || err.response?.data?.message);
  }

  console.log('\nTrying /lite/v1/positions...');
  try {
    const resp = await axios.post(`${tradesUrl}/lite/v1/positions`, { sa: '0' }, { headers });
    console.log('✅ positions works:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.log('❌ positions failed:', err.response?.status, err.response?.data?.m || err.response?.data?.message);
  }

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
