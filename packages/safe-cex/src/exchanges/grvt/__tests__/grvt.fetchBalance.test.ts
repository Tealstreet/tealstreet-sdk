/**
 * @vitest-environment node
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';

/**
 * Standalone test for GRVT fetchBalance with new credentials
 * This test verifies that authentication and balance fetching work correctly
 */

describe('GRVT fetchBalance Test', () => {
  let exchange: GrvtExchange | null = null;
  let store: DefaultStore;
  let credentials: {
    apiKey?: string;
    secret?: string;
    testnet?: boolean;
    subaccountId?: string;
    grvtSignerAddress?: string;
  } = {};

  beforeAll(async () => {
    // Load credentials from .apikey.json
    const credPath = resolve(__dirname, '../.apikey.json');

    if (!existsSync(credPath)) {
      console.log('⚠️ No credentials file found at:', credPath);
      console.log('Please create .apikey.json with your GRVT credentials');
      return;
    }

    const credData = readFileSync(credPath, 'utf-8');
    const config = JSON.parse(credData);

    credentials = {
      apiKey: config.apiKey,
      secret: config.secret,
      testnet: config.testnet !== undefined ? config.testnet : false,
      subaccountId: config.subaccountId,
      grvtSignerAddress: config.grvtSignerAddress,
    };

    console.log('\n==============================');
    console.log('GRVT fetchBalance Test');
    console.log('==============================');
    console.log('API Key:', credentials.apiKey?.substring(0, 10) + '...');
    console.log('Testnet:', credentials.testnet);
    console.log('Sub-Account ID:', credentials.subaccountId);
    console.log('Signer Address:', credentials.grvtSignerAddress);
    console.log('==============================\n');

    // Create store
    store = new DefaultStore();

    // Create exchange instance
    exchange = new GrvtExchange(
      'grvt',
      {
        key: credentials.apiKey!,
        secret: credentials.secret!,
        testnet: credentials.testnet,
        subaccount: credentials.subaccountId,
        grvtSignerAddress: credentials.grvtSignerAddress,
      },
      store
    );

    // Allow some time for initialization
    await new Promise(resolve => setTimeout(resolve, 1000));
  }, 30000);

  afterAll(() => {
    if (exchange) {
      exchange.dispose();
      exchange = null;
    }
  });

  it('should authenticate successfully', async () => {
    if (!credentials.apiKey) {
      console.log('⚠️ Skipping test - no credentials found');
      return;
    }

    expect(exchange).toBeDefined();
    expect(exchange).not.toBeNull();

    console.log('Testing authentication...');
    const result = await exchange!.validateAccount();

    console.log('Authentication result:', result === '' ? '✅ Success' : `❌ ${result}`);

    if (result !== '') {
      console.error('Authentication failed:', result);
      throw new Error(`Authentication failed: ${result}`);
    }

    expect(result).toBe('');
    expect(exchange!.apiExtras).toBeDefined();
    expect(exchange!.apiExtras?.accountId).toBeDefined();
    expect(exchange!.apiExtras?.sessionCookie).toBeDefined();

    console.log('Account ID:', exchange!.apiExtras?.accountId);
    console.log('Session established: ✅');
  }, 60000);

  it('should fetch balance without 401/403 errors', async () => {
    if (!credentials.apiKey || !exchange) {
      console.log('⚠️ Skipping test - no credentials or exchange not initialized');
      return;
    }

    console.log('\nTesting fetchBalance...');

    // Ensure we're authenticated
    if (!exchange.apiExtras?.sessionCookie) {
      const authResult = await exchange.validateAccount();
      if (authResult !== '') {
        throw new Error(`Failed to authenticate: ${authResult}`);
      }
    }

    // Fetch balance
    const balance = await exchange.fetchBalance();

    console.log('\n✅ Balance fetched successfully!');
    console.log('Total Collateral:', `$${balance.total.toFixed(2)}`);
    console.log('Free:', `$${balance.free.toFixed(2)}`);
    console.log('Used:', `$${balance.used.toFixed(2)}`);
    console.log('UPNL:', `$${balance.upnl.toFixed(2)}`);

    // Verify balance structure
    expect(balance).toBeDefined();
    expect(typeof balance.total).toBe('number');
    expect(typeof balance.free).toBe('number');
    expect(typeof balance.used).toBe('number');
    expect(typeof balance.upnl).toBe('number');
    expect(balance.currencies).toBeDefined();
    expect(typeof balance.currencies).toBe('object');

    // Values should be >= 0
    expect(balance.total).toBeGreaterThanOrEqual(0);
    expect(balance.free).toBeGreaterThanOrEqual(0);
    expect(balance.used).toBeGreaterThanOrEqual(0);

    // Log currencies
    if (balance.currencies) {
      const currencies = Object.keys(balance.currencies);
      console.log(`\nCurrencies (${currencies.length}):`);

      for (const currency of currencies) {
        const bal = balance.currencies[currency];
        if (bal && ((bal as any).available > 0 || (bal as any).locked > 0)) {
          console.log(`  ${currency}:`, {
            available: (bal as any).available,
            locked: (bal as any).locked,
          });
        }
      }
    }

    console.log('\n✅ fetchBalance test completed successfully!\n');
  }, 60000);

  it('should verify no authentication errors', async () => {
    if (!credentials.apiKey || !exchange) {
      console.log('⚠️ Skipping test - no credentials or exchange not initialized');
      return;
    }

    // Make sure session is still valid
    expect(exchange.apiExtras?.sessionCookie).toBeDefined();
    expect(exchange.apiExtras?.accountId).toBeDefined();

    console.log('\n✅ No 401/403 authentication errors detected');
    console.log('Session is valid and working correctly\n');
  }, 10000);
});
