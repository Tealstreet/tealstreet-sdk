#!/usr/bin/env tsx
import { existsSync, readFileSync } from 'fs';
import { resolve } from 'path';

import { Wallet } from 'ethers';

import { domain as baseDomain, Signer as GrvtSigner, Order as OrderTypes } from '@tealstreet/grvt-ts-sdk';
import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function tryVariation(
  variationName: string,
  grvtClient: any,
  wallet: Wallet,
  privateKey: string,
  tradingAccountId: string,
  instrument: any,
  modifyMessage: (msg: any) => any,
  modifyDomain?: (domain: any) => any,
) {
  try {
    console.log(`\n=== Trying: ${variationName} ===`);

    const price = 70000;
    const size = 0.001;
    const nonce = Math.floor(Math.random() * 1e9);
    const expirationStr = ((Date.now() + 3600 * 1000) * 1e6).toString();

    const baseDecimals = instrument.bd || 9;
    const PRICE_MULTIPLIER = 1_000_000_000;
    const assetID = parseInt(instrument.ih, 16);

    let eip712Message: any = {
      subAccountID: parseInt(tradingAccountId, 10),
      isMarket: false,
      timeInForce: 0, // GOOD_TILL_TIME
      postOnly: false,
      reduceOnly: false,
      legs: [
        {
          assetID: assetID,
          contractSize: Math.floor(size * Math.pow(10, baseDecimals)),
          limitPrice: Math.floor(price * PRICE_MULTIPLIER),
          isBuyingContract: true,
        },
      ],
      nonce,
      expiration: expirationStr,
    };

    // Apply modification
    eip712Message = modifyMessage(eip712Message);

    let domain = {
      ...baseDomain,
      chainId: 325,
    };

    if (modifyDomain) {
      domain = modifyDomain(domain);
    }

    const typedData = {
      ...OrderTypes,
      domain,
      message: eip712Message,
    };

    const rawSignature = GrvtSigner.sign(privateKey, typedData);
    const { r, s, v } = GrvtSigner.decode(rawSignature);

    const fullFormatOrder = {
      sub_account_id: tradingAccountId,
      is_market: false,
      time_in_force: 'GOOD_TILL_TIME',
      post_only: false,
      reduce_only: false,
      legs: [
        {
          instrument: 'BTC_USDT_Perp',
          size: size.toString(),
          limit_price: price.toString(),
          is_buying_asset: true,
        },
      ],
      signature: {
        signer: wallet.address,
        r: r,
        s: s,
        v: v,
        expiration: expirationStr,
        nonce: nonce,
        chain_id: domain.chainId.toString(),
      },
    };

    const response = await grvtClient.tdgClient.createOrder(
      { order: fullFormatOrder },
      await grvtClient.authenticatedEndpoint(),
    );

    if (response.result) {
      console.log(`✅ SUCCESS! Order placed: ${response.result.order_id}`);
      console.log(`Variation: ${variationName}`);
      return true;
    }
  } catch (err: any) {
    const errorCode = err.response?.data?.code;
    console.log(`❌ Failed: ${errorCode} - ${err.response?.data?.message || err.message}`);
  }

  return false;
}

async function main() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));
  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: config.apiKey,
      secret: config.secret,
      testnet: config.testnet || false,
      subaccount: config.subaccountId,
      grvtSignerAddress: config.grvtSignerAddress,
      grvtTradingAccountId: config.grvtTradingAccountId,
    },
    store,
  );

  await exchange.authenticate();
  const markets = await exchange.fetchMarkets();
  store.update({ markets });

  const instResponse = (await (exchange as any).xhr.post('/lite/v1/all_instruments', {})) as { data?: { r: any[] } };
  const instrument = instResponse.data?.r?.find((i: any) => i.i === 'BTC_USDT_Perp');

  const grvtClient = (exchange as any).grvtClient;
  const wallet = (exchange as any).wallet;
  const privateKey = config.secret?.startsWith('0x') ? config.secret : `0x${config.secret}`;
  const tradingAccountId = config.grvtTradingAccountId;

  console.log('Testing different variations...\n');

  // Test 1: Current implementation
  if (
    await tryVariation(
      'Current (chainId=325)',
      grvtClient,
      wallet,
      privateKey,
      tradingAccountId,
      instrument,
      (msg) => msg,
    )
  ) {
    exchange.dispose();
    return;
  }

  // Test 2: Try chainId 324
  if (
    await tryVariation(
      'chainId=324',
      grvtClient,
      wallet,
      privateKey,
      tradingAccountId,
      instrument,
      (msg) => msg,
      (domain) => ({ ...domain, chainId: 324 }),
    )
  ) {
    exchange.dispose();
    return;
  }

  // Test 3: Try chainId 326
  if (
    await tryVariation(
      'chainId=326',
      grvtClient,
      wallet,
      privateKey,
      tradingAccountId,
      instrument,
      (msg) => msg,
      (domain) => ({ ...domain, chainId: 326 }),
    )
  ) {
    exchange.dispose();
    return;
  }

  // Test 4: Try expiration as number
  if (
    await tryVariation('expiration as number', grvtClient, wallet, privateKey, tradingAccountId, instrument, (msg) => ({
      ...msg,
      expiration: parseInt(msg.expiration, 10),
    }))
  ) {
    exchange.dispose();
    return;
  }

  // Test 5: Try subAccountID as string
  if (
    await tryVariation(
      'subAccountID as string',
      grvtClient,
      wallet,
      privateKey,
      tradingAccountId,
      instrument,
      (msg) => ({ ...msg, subAccountID: tradingAccountId }),
    )
  ) {
    exchange.dispose();
    return;
  }

  // Test 6: Try assetID as string (hex)
  if (
    await tryVariation(
      'assetID as hex string',
      grvtClient,
      wallet,
      privateKey,
      tradingAccountId,
      instrument,
      (msg) => ({
        ...msg,
        legs: msg.legs.map((leg: any) => ({ ...leg, assetID: instrument.ih })),
      }),
    )
  ) {
    exchange.dispose();
    return;
  }

  // Test 7: Try zero subAccountID
  if (
    await tryVariation('subAccountID=0', grvtClient, wallet, privateKey, tradingAccountId, instrument, (msg) => ({
      ...msg,
      subAccountID: 0,
    }))
  ) {
    exchange.dispose();
    return;
  }

  // Test 8: Try empty string subAccountID
  if (
    await tryVariation('subAccountID=""', grvtClient, wallet, privateKey, tradingAccountId, instrument, (msg) => ({
      ...msg,
      subAccountID: '',
    }))
  ) {
    exchange.dispose();
    return;
  }

  console.log('\n❌ All variations failed');
  exchange.dispose();
}

main().catch(console.error);
