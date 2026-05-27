/**
 * Test order placement using tradingXhr directly (bypassing SDK)
 *
 * Run with: npx tsx src/exchanges/grvt/test-order-direct-xhr.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { Wallet } from 'ethers';
import { signTypedData, SignTypedDataVersion } from '@metamask/eth-sig-util';

const credPath = path.resolve(__dirname, '.apikey.json');

async function main() {
  console.log('=== GRVT Order Test - Direct XHR ===\n');

  if (!fs.existsSync(credPath)) {
    console.log('⚠️ No credentials file found');
    process.exit(0);
  }

  const config = JSON.parse(fs.readFileSync(credPath, 'utf-8'));

  console.log('Config:');
  console.log('  API Key:', config.apiKey?.substring(0, 15) + '...');

  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: config.apiKey,
      secret: config.apiSecret,
      testnet: config.testnet || false,
    },
    store
  );

  try {
    console.log('\n1. Authenticating...');
    await exchange.authenticate();
    console.log('   ✅ Authenticated');
    console.log('   Session cookie:', (exchange as any).apiExtras?.sessionCookie?.substring(0, 50) + '...');
    console.log('   Account ID:', (exchange as any).apiExtras?.accountId);

    console.log('\n2. Fetching markets...');
    const markets = await exchange.fetchMarkets();
    store.update({ markets });
    console.log('   ✅ Fetched', markets.length, 'markets');

    // Get BTC market
    const btcMarket = markets.find((m) => m.symbol.includes('BTC') && m.symbol.includes('PERP'));
    if (!btcMarket) {
      console.log('   ❌ No BTC market found');
      return;
    }

    // Get the wallet
    const wallet = (exchange as any).wallet as Wallet;
    if (!wallet) {
      console.log('   ❌ No wallet initialized');
      return;
    }
    console.log('   Signer:', wallet.address);

    // Build and sign order
    console.log('\n3. Building order...');

    const now = Date.now();
    const nonce = Math.floor(Math.random() * 1000000);
    const expiration = (now + 3600000) * 1000000;

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

    const instrumentHash = '0x030501';
    const baseDecimals = 9;
    const PRICE_MULTIPLIER = 1_000_000_000;
    const size = 0.001;
    const price = 50000;
    const contractSize = Math.floor(size * Math.pow(10, baseDecimals));
    const limitPrice = Math.floor(price * PRICE_MULTIPLIER);

    // Use 0 as sub_account_id
    const subAccountId = 0;

    const eip712Message = {
      subAccountID: subAccountId,
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

    // Build full format order
    const fullOrder = {
      order: {
        sub_account_id: String(subAccountId),
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

    console.log('   Order:', JSON.stringify(fullOrder, null, 2));

    // Send order using tradingXhr directly
    console.log('\n4. Sending order via tradingXhr...');
    const tradingXhr = (exchange as any).tradingXhr;

    try {
      const response = await tradingXhr.post('/full/v1/create_order', fullOrder);
      console.log('   ✅ SUCCESS! Response:', JSON.stringify(response.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ FAILED:', err.response?.status);
      console.log('   Error:', JSON.stringify(err.response?.data, null, 2));

      // Check if it's a specific error we can learn from
      const errData = err.response?.data;
      if (errData?.code || errData?.c) {
        console.log('\n   Error code:', errData.code || errData.c);
        console.log('   Message:', errData.message || errData.m);
      }
    }

    // Also try lite format
    console.log('\n5. Trying lite format via tradingXhr...');
    const liteOrder = {
      o: {
        sa: String(subAccountId),
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
      const response = await tradingXhr.post('/lite/v1/create_order', liteOrder);
      console.log('   ✅ SUCCESS! Response:', JSON.stringify(response.data, null, 2));
    } catch (err: any) {
      console.log('   ❌ FAILED:', err.response?.status);
      console.log('   Error:', JSON.stringify(err.response?.data, null, 2));
    }

  } catch (err: any) {
    console.log('\n❌ Error:', err.message);
  } finally {
    exchange.dispose();
  }

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
