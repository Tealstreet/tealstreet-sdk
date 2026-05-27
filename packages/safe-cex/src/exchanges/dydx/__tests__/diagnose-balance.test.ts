import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import axios from 'axios';

/**
 * Diagnostic test to check dYdX API responses for the configured wallet
 */

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

describe('DydxExchange - API Diagnostics', () => {
  let apiConfig: ApiKeyConfig;
  let baseUrl: string;
  let address: string;
  let subaccountNumber: number;

  beforeAll(async () => {
    // Load API credentials from .apikey.json
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    baseUrl = apiConfig.testnet
      ? 'https://indexer.v4testnet.dydx.exchange'
      : 'https://indexer.dydx.trade';
    address = apiConfig.apiKey;
    subaccountNumber = 0;

    console.log('\n=== dYdX API Diagnostics ===');
    console.log('Wallet Address:', address);
    console.log('Network:', apiConfig.testnet ? 'Testnet' : 'Mainnet');
    console.log('Base URL:', baseUrl);
    console.log('Subaccount:', subaccountNumber);
    console.log('===========================\n');
  });

  it('should check if address has any subaccounts', async () => {
    const url = `${baseUrl}/v4/addresses/${address}`;
    console.log(`\n[1] GET ${url}`);

    try {
      const response = await axios.get(url);
      console.log('✓ Response Status:', response.status);
      console.log('✓ Subaccounts:', JSON.stringify(response.data, null, 2));
      expect(response.status).toBe(200);
    } catch (error: any) {
      console.log('✗ Error Status:', error.response?.status);
      console.log('✗ Error Data:', JSON.stringify(error.response?.data, null, 2));

      if (error.response?.status === 404) {
        console.log('⚠ Address not found - account may be empty or new');
      }

      // Don't fail the test on 404 - it's expected for empty accounts
      expect([200, 404]).toContain(error.response?.status);
    }
  });

  it('should check specific subaccount', async () => {
    const url = `${baseUrl}/v4/addresses/${address}/subaccountNumber/${subaccountNumber}`;
    console.log(`\n[2] GET ${url}`);

    try {
      const response = await axios.get(url);
      console.log('✓ Response Status:', response.status);
      console.log('✓ Subaccount Data:', JSON.stringify(response.data, null, 2));
      expect(response.status).toBe(200);
    } catch (error: any) {
      console.log('✗ Error Status:', error.response?.status);
      console.log('✗ Error Data:', JSON.stringify(error.response?.data, null, 2));

      if (error.response?.status === 404) {
        console.log('⚠ Subaccount not found - may be empty');
      }

      // Don't fail the test on 404
      expect([200, 404]).toContain(error.response?.status);
    }
  });

  it('should check asset positions (USDC balance)', async () => {
    const url = `${baseUrl}/v4/assetPositions`;
    const params = {
      address,
      subaccountNumber: subaccountNumber.toString(),
    };
    console.log(`\n[3] GET ${url}?${new URLSearchParams(params as any).toString()}`);

    try {
      const response = await axios.get(url, { params });
      console.log('✓ Response Status:', response.status);
      console.log('✓ Asset Positions:', JSON.stringify(response.data, null, 2));
      expect(response.status).toBe(200);

      if (response.data && response.data.positions) {
        const usdcPosition = response.data.positions.find(
          (p: any) => p.symbol === 'USDC' || p.assetId === '0'
        );
        if (usdcPosition) {
          console.log('\n💰 USDC Balance found:', usdcPosition.size);
        } else {
          console.log('\n⚠ No USDC balance found in positions');
        }
      }
    } catch (error: any) {
      console.log('✗ Error Status:', error.response?.status);
      console.log('✗ Error Data:', JSON.stringify(error.response?.data, null, 2));

      if (error.response?.status === 404) {
        console.log('⚠ No asset positions found - account has zero balance');
      }

      // Don't fail the test on 404
      expect([200, 404]).toContain(error.response?.status);
    }
  });

  it('should check perpetual positions', async () => {
    const url = `${baseUrl}/v4/perpetualPositions`;
    const params = {
      address,
      subaccountNumber: subaccountNumber.toString(),
    };
    console.log(`\n[4] GET ${url}?${new URLSearchParams(params as any).toString()}`);

    try {
      const response = await axios.get(url, { params });
      console.log('✓ Response Status:', response.status);
      console.log('✓ Perpetual Positions:', JSON.stringify(response.data, null, 2));
      expect(response.status).toBe(200);

      if (response.data && response.data.positions) {
        console.log(`\n📊 Found ${response.data.positions.length} perpetual position(s)`);
      }
    } catch (error: any) {
      console.log('✗ Error Status:', error.response?.status);
      console.log('✗ Error Data:', JSON.stringify(error.response?.data, null, 2));

      if (error.response?.status === 404) {
        console.log('⚠ No perpetual positions found');
      }

      // Don't fail the test on 404
      expect([200, 404]).toContain(error.response?.status);
    }
  });

  it('should check open orders', async () => {
    const url = `${baseUrl}/v4/orders`;
    const params = {
      address,
      subaccountNumber: subaccountNumber.toString(),
    };
    console.log(`\n[5] GET ${url}?${new URLSearchParams(params as any).toString()}`);

    try {
      const response = await axios.get(url, { params });
      console.log('✓ Response Status:', response.status);
      console.log('✓ Orders:', JSON.stringify(response.data, null, 2));
      expect(response.status).toBe(200);

      if (Array.isArray(response.data)) {
        console.log(`\n📝 Found ${response.data.length} order(s)`);
      }
    } catch (error: any) {
      console.log('✗ Error Status:', error.response?.status);
      console.log('✗ Error Data:', JSON.stringify(error.response?.data, null, 2));

      if (error.response?.status === 404) {
        console.log('⚠ No orders found');
      }

      // Don't fail the test on 404
      expect([200, 404]).toContain(error.response?.status);
    }
  });

  it('should check fills (trade history)', async () => {
    const url = `${baseUrl}/v4/fills`;
    const params = {
      address,
      subaccountNumber: subaccountNumber.toString(),
    };
    console.log(`\n[6] GET ${url}?${new URLSearchParams(params as any).toString()}`);

    try {
      const response = await axios.get(url, { params });
      console.log('✓ Response Status:', response.status);
      console.log('✓ Fills:', JSON.stringify(response.data, null, 2));
      expect(response.status).toBe(200);

      if (response.data && response.data.fills) {
        console.log(`\n📈 Found ${response.data.fills.length} fill(s)`);
      }
    } catch (error: any) {
      console.log('✗ Error Status:', error.response?.status);
      console.log('✗ Error Data:', JSON.stringify(error.response?.data, null, 2));

      if (error.response?.status === 404) {
        console.log('⚠ No fills found');
      }

      // Don't fail the test on 404
      expect([200, 404]).toContain(error.response?.status);
    }
  });

  it('should summarize findings', () => {
    console.log('\n');
    console.log('================================');
    console.log('DIAGNOSTIC SUMMARY');
    console.log('================================');
    console.log('If all endpoints returned 404:');
    console.log('  → Account exists but has no activity (empty account)');
    console.log('  → You may need to deposit USDC or trade to see balances');
    console.log('');
    console.log('The current implementation handles 404s gracefully,');
    console.log('returning zero balances for empty accounts.');
    console.log('================================\n');

    expect(true).toBe(true);
  });
});
