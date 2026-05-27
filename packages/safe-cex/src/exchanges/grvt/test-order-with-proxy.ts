/**
 * Test order placement using the Tealstreet private proxy
 *
 * Run with: npx tsx src/exchanges/grvt/test-order-with-proxy.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import axios from 'axios';
import { Wallet } from 'ethers';
import { signTypedData, SignTypedDataVersion } from '@metamask/eth-sig-util';

const credPath = path.resolve(__dirname, '.apikey.json');

// Tealstreet proxy configuration (GRVT uses the tiny proxy)
const PROXY_URL = 'https://fsjnpy143pbz-tiny.jp.tealstreet.io/api/exchange_proxy';
const PROXY_TOKEN = 'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VySWQiOiJvYXV0aDJ8ZGlzY29yZHwyMzIyMjQ0NjA2MDA1MDg0MTYiLCJleHAiOjE3NjcwOTY1NzEsImlhdCI6MTc2NjQ5MTc3MX0.c64YjNnkYtvK3uuvHlspDtQt_niQnA7Mtu7l_nJYwS4AlfEt-_j4ITlgCzjODqOmV-Sd62MynnTnZxRUYyjwdvOfHnp8m7UHHj5P7WHzn6pcSrdk75zLBNrI8tf6YOuBw0SzZdrI_hA6oBQWI8xI2lr6nPOwB0ppXjMtJN7-QIGqnwglHYeJz8uUIYKtiPwzCua10C1pks9CaIKB9zr89yqYQY4BxyXJ4hyWtrEvq2xueJfR_fQUAmxatnJ36Ds1OqcbvBdUOpSTCmoSYR6DUQT_EhNRXq5nfje8wbdRBAmbuRQhx8J5DoFozoet63bxMzEVug_x9WQcRpVXsCf34vod-JfTBwAb9-SuoYR_lWOUgOQ_v7SiVwIRZxI3_rfcRxXX1S4mTcQ6KV31jPh4E2mxdrTlCnHYvUqT9fnMozZ0UnkGITSnIxRFgs_vvChAPocOeL7PgrN5LBWn1o11XlSrrsFVty9u1YOlSr4keOjAOQJu0PzpUdI8kceh5Hr7ITLbj0yNNzKID0guc7cNkRMGczrkPFopC8FMG9ckDQagFWYkDMVCBwIJxeiZvbj1iRPV-mO8MSiBZZFvdtXd5L0n_W1K_fgYVWsMe7pdW8Db60-2UAYTH8MwyOhw7EoEHDHO3Ywfd5RLFWWctyPqVqFX3JTuoq8520O0dC7L3hY';

async function main() {
  console.log('=== GRVT Order Test - With Proxy ===\n');

  const config = JSON.parse(fs.readFileSync(credPath, 'utf-8'));
  const isTestnet = config.testnet ?? false;

  // Use proxy for all requests
  const edgeUrl = isTestnet
    ? `${PROXY_URL}/https://edge.testnet.grvt.io`
    : `${PROXY_URL}/https://edge.grvt.io`;
  const tradesUrl = isTestnet
    ? `${PROXY_URL}/https://trades.testnet.grvt.io`
    : `${PROXY_URL}/https://trades.grvt.io`;

  console.log('Using proxy:', PROXY_URL);
  console.log('Edge URL:', edgeUrl);
  console.log('Trades URL:', tradesUrl);

  // Initialize wallet
  const privateKey = config.apiSecret.startsWith('0x') ? config.apiSecret : `0x${config.apiSecret}`;
  const wallet = new Wallet(privateKey);
  console.log('Wallet/Signer address:', wallet.address);

  // Common headers for proxy requests - must include browser-like headers
  const proxyHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    'Tealstreet-Authorization': `Bearer ${PROXY_TOKEN}`,
    'Origin': 'https://trade.tealstreet.io',
    'Referer': 'https://trade.tealstreet.io/',
    'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
    'sec-fetch-mode': 'cors',
    'sec-fetch-site': 'cross-site',
    'sec-fetch-dest': 'empty',
    'Accept': 'application/json, text/plain, */*',
  };

  // Authenticate via proxy
  console.log('\nAuthenticating via proxy...');
  const authResponse = await axios.post(
    `${edgeUrl}/auth/api_key/login`,
    { api_key: config.apiKey },
    {
      headers: {
        ...proxyHeaders,
        'X-Grvt-Remember-Me': 'true',
      },
    }
  );

  console.log('Auth response status:', authResponse.status);
  console.log('Auth response headers:', Object.keys(authResponse.headers));

  // Get session cookie from proxy response
  // Proxy should expose it via X-Ts-Grvt-Session header
  const proxySessionCookie = authResponse.headers['x-ts-grvt-session'];
  const accountIdHeader = authResponse.headers['x-grvt-account-id'];

  // Also try Set-Cookie (proxy might pass it through)
  let sessionCookie = proxySessionCookie;
  if (!sessionCookie) {
    const cookies = authResponse.headers['set-cookie'];
    if (cookies) {
      for (const cookie of cookies) {
        if (cookie.includes('gravity=')) {
          sessionCookie = cookie.split(';')[0].split('=')[1];
          break;
        }
      }
    }
  }

  console.log('Session cookie:', sessionCookie ? sessionCookie.substring(0, 30) + '...' : '(none)');
  console.log('x-grvt-account-id:', accountIdHeader);

  if (!sessionCookie) {
    console.log('❌ No session cookie received from proxy');
    console.log('Full response headers:', JSON.stringify(authResponse.headers, null, 2));
    return;
  }

  // Headers for trading requests (with session)
  const tradingHeaders: Record<string, string> = {
    ...proxyHeaders,
    'X-Ts-Grvt-Session': sessionCookie,
  };
  if (accountIdHeader) {
    tradingHeaders['X-Grvt-Account-Id'] = accountIdHeader;
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

  // Build lite format order
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

  console.log('\nOrder details:');
  console.log('  sub_account_id:', subAccountId);
  console.log('  signer:', wallet.address);
  console.log('  instrument: BTC_USDT_Perp');
  console.log('  size: 0.001');
  console.log('  price: 50000');

  // First test: check if we can access open_orders
  console.log('\n--- Testing open_orders via proxy ---');
  try {
    const resp = await axios.post(
      `${tradesUrl}/lite/v1/open_orders`,
      { sa: '0' },
      { headers: tradingHeaders }
    );
    console.log('✅ open_orders works:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.log('❌ open_orders failed:', err.response?.status, err.response?.data?.m || err.response?.data?.message || err.message);
  }

  // Second test: check aggregated_account_summary
  console.log('\n--- Testing aggregated_account_summary via proxy ---');
  try {
    const resp = await axios.post(
      `${tradesUrl}/lite/v1/aggregated_account_summary`,
      {},
      { headers: tradingHeaders }
    );
    console.log('✅ aggregated_account_summary works:', JSON.stringify(resp.data, null, 2));
  } catch (err: any) {
    console.log('❌ aggregated_account_summary failed:', err.response?.status, err.response?.data?.m || err.response?.data?.message || err.message);
  }

  // Third test: try to place order
  console.log('\n--- Trying order placement via proxy ---');
  try {
    const response = await axios.post(
      `${tradesUrl}/lite/v1/create_order`,
      liteOrder,
      { headers: tradingHeaders }
    );
    console.log('✅ SUCCESS! Response:', JSON.stringify(response.data, null, 2));
  } catch (err: any) {
    console.log('❌ FAILED:', err.response?.status);
    console.log('Error:', JSON.stringify(err.response?.data, null, 2));
  }

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
