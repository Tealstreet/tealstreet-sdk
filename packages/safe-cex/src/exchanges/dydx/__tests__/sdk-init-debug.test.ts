/**
 * @vitest-environment node
 */

import { readFileSync } from 'fs';
import { join } from 'path';

import { CompositeClient, LocalWallet, Network } from '@dydxprotocol/v4-client-js';
import { beforeAll, describe, expect, it } from 'vitest';

describe('dYdX SDK Initialization Debug', () => {
  beforeAll(async () => {
    // Setup fetch polyfill if not available (Node.js 22+ has native fetch)
    if (typeof globalThis.fetch === 'undefined') {
      // @ts-expect-error - node-fetch has no type declarations in this context
      const nodeFetch = await import('node-fetch');
      (globalThis as any).fetch = nodeFetch.default;
      (globalThis as any).Headers = nodeFetch.Headers;
      (globalThis as any).Request = nodeFetch.Request;
      (globalThis as any).Response = nodeFetch.Response;
    }
  });

  it('should test SDK components individually', async () => {
    // Load API credentials
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    const apiConfig = JSON.parse(configData);

    console.log('\n1. Testing Network creation...');
    const network = apiConfig.testnet ? Network.testnet() : Network.mainnet();
    console.log('✅ Network created:', {
      indexerRestUrl: network.indexerConfig?.restEndpoint,
      chainId: network.validatorConfig?.chainId,
    });

    console.log('\n2. Testing LocalWallet.fromPrivateKey...');
    const pkHex = apiConfig.secret.startsWith('0x') ? apiConfig.secret.slice(2) : apiConfig.secret;
    try {
      // Check if fromPrivateKey is async or sync
      console.log('   Calling LocalWallet.fromPrivateKey...');
      const wallet = await LocalWallet.fromPrivateKey(pkHex, 'dydx');
      console.log('✅ Wallet created:', wallet.address);

      // Verify it matches expected address
      if (wallet.address === apiConfig.apiKey) {
        console.log('✅ Wallet address matches API key');
      } else {
        console.log('⚠️  Wallet address mismatch:');
        console.log('   Expected:', apiConfig.apiKey);
        console.log('   Got:', wallet.address);
      }
    } catch (err: any) {
      console.error('❌ Error creating wallet:', err.message);
      throw err;
    }

    console.log('\n3. Testing CompositeClient.connect...');
    try {
      console.log('   Calling CompositeClient.connect...');
      const client = await CompositeClient.connect(network);
      console.log('✅ CompositeClient connected');
      console.log('   Has indexerClient?', !!client.indexerClient);
      console.log('   Has validatorClient?', !!client.validatorClient);
    } catch (err: any) {
      console.error('❌ Error connecting CompositeClient:', err.message);
      throw err;
    }

    console.log('\n✅ All SDK components initialized successfully\n');
  }, 60000);
});
