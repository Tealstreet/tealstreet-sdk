/**
 * @vitest-environment node
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync } from 'fs';
import { TDG } from '@tealstreet/grvt-client';
import { Signer as GrvtSigner, Order as OrderTypes, domain as baseDomain } from '@tealstreet/grvt-ts-sdk';
import { BASE_URLS } from '../grvt.types';

// Local type definitions matching the GRVT client types
type IOrder = {
  sub_account_id: string;
  is_market: boolean;
  time_in_force: string;
  post_only: boolean;
  reduce_only: boolean;
  legs: Array<{
    instrument: string;
    size: string;
    limit_price: string;
    is_buying_asset: boolean;
  }>;
  signature: {
    signer: string;
    r: string;
    s: string;
    v: number;
    expiration: string;
    nonce: number;
    chain_id: string;
  };
};

type IApiCreateOrderRequest = {
  order: IOrder;
};

/**
 * Test placing orders on GRVT using the SDK
 */
describe('GRVT SDK Place Order Test', () => {
  let tdg: TDG | null = null;
  let credentials: {
    apiKey?: string;
    secret?: string;
    testnet?: boolean;
    accountId?: string;
    subAccountId?: string;
  } = {};
  let privateKey: string = '';

  beforeAll(async () => {
    // Try to load credentials from .apikey.json
    const credPath = resolve(__dirname, '../.apikey.json');
    if (existsSync(credPath)) {
      try {
        credentials = JSON.parse(readFileSync(credPath, 'utf-8'));
        console.log('✅ Loaded API credentials from .apikey.json');

        // Ensure private key starts with 0x
        privateKey = credentials.secret?.startsWith('0x')
          ? credentials.secret
          : `0x${credentials.secret}`;
      } catch (error) {
        console.log('⚠️ Error loading credentials:', error);
      }
    }

    // Initialize TDG client
    const baseURL =
      credentials.testnet !== false ? BASE_URLS.TESTNET_TDG : BASE_URLS.PRODUCTION_TDG;

    tdg = new TDG({
      host: baseURL,
    });
  });

  it('should have valid credentials', () => {
    expect(credentials.apiKey).toBeDefined();
    expect(credentials.secret).toBeDefined();
    // subAccountId is optional - defaults to empty string which GRVT treats as default sub-account
  });

  it('should place a limit order using SDK', async () => {
    if (!tdg) {
      throw new Error('TDG not initialized');
    }

    // Use provided subAccountId or empty string (GRVT default sub-account)
    const subAccountId = credentials.subAccountId || '';

    // Generate nonce and expiration
    const nonce = Math.floor(Math.random() * 0xffffffff); // uint32 max value
    const expiration = (Date.now() * 1_000_000 + 3600 * 1_000_000_000).toString(); // 1 hour from now in nanoseconds

    // Prepare EIP-712 domain
    const chainId = credentials.testnet !== false ? 325 : 324; // zkSync testnet = 325, mainnet = 324
    const domain = {
      ...baseDomain,
      chainId,
    };

    // Create EIP-712 message for signing (camelCase format matching Order type definition)
    const eip712Message = {
      subAccountID: subAccountId,
      isMarket: false,
      timeInForce: 0, // GOOD_TILL_TIME enum value
      postOnly: false,
      reduceOnly: false,
      legs: [
        {
          assetID: '0', // Will be mapped by GRVT from instrument
          contractSize: '1000', // 0.001 BTC = 1000 units
          limitPrice: '10000000000', // $10,000 (9 decimals)
          isBuyingContract: true,
        },
      ],
      nonce,
      expiration,
    };

    // Prepare typed data for signing
    const typedData = {
      ...OrderTypes,
      domain,
      message: eip712Message,
    };

    console.log('\n📝 Signing order...');
    console.log(`   Instrument: BTC_USDT_Perp`);
    console.log(`   Size: 0.001 BTC`);
    console.log(`   Price: $10,000`);
    console.log(`   Side: BUY`);
    console.log(`   Nonce: ${nonce}`);

    // Sign the order
    const rawSignature = GrvtSigner.sign(privateKey, typedData);
    const { r, s, v } = GrvtSigner.decode(rawSignature);

    console.log(`\n✅ Signature generated:`);
    console.log(`   r: ${r}`);
    console.log(`   s: ${s}`);
    console.log(`   v: ${v}`);

    // Get signer address from private key
    const { Wallet } = await import('ethers');
    const wallet = new Wallet(privateKey);
    const signerAddress = wallet.address;

    // Build complete order with signature (snake_case format for API)
    const order: IOrder = {
      sub_account_id: subAccountId,
      is_market: false,
      time_in_force: 'GOOD_TILL_TIME',
      post_only: false,
      reduce_only: false,
      legs: [
        {
          instrument: 'BTC_USDT_Perp',
          size: '0.001', // 0.001 BTC
          limit_price: '10000000000', // $10,000 (9 decimals)
          is_buying_asset: true,
        },
      ],
      signature: {
        signer: signerAddress,
        r,
        s,
        v,
        expiration,
        nonce,
        chain_id: chainId.toString(),
      },
    };

    const orderRequest: IApiCreateOrderRequest = {
      order,
    };

    console.log(`\n🚀 Placing order via SDK...`);

    // Place order using SDK
    const response = await tdg.createOrder(orderRequest as any);

    console.log(`\n✅ Order placed successfully!`);
    console.log(`   Order ID: ${response.result?.order_id}`);
    console.log(`   Status: ${response.result?.state?.status}`);

    expect(response.result).toBeDefined();
    expect(response.result?.order_id).toBeDefined();
    expect(typeof response.result?.order_id).toBe('string');

    // Store order ID for cleanup
    (tdg as any).testOrderId = response.result?.order_id;
  }, 30000);

  it('should cancel the test order', async () => {
    if (!tdg) {
      throw new Error('TDG not initialized');
    }

    const testOrderId = (tdg as any).testOrderId;
    if (!testOrderId) {
      console.log('⚠️ No test order ID found, skipping...');
      return;
    }

    const subAccountId = credentials.subAccountId || '';

    console.log(`\n🗑️ Cancelling order ${testOrderId}...`);

    await tdg.cancelOrder({
      sub_account_id: subAccountId,
      order_id: testOrderId,
    });

    console.log(`✅ Order cancelled successfully`);
  }, 30000);
});
