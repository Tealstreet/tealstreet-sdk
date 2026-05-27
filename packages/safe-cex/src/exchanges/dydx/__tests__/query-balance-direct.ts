#!/usr/bin/env ts-node

import { readFileSync } from 'fs';
import { join } from 'path';
import axios from 'axios';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

async function main() {
  // Load API credentials
  const configPath = join(__dirname, '..', '.apikey.json');
  const configData = readFileSync(configPath, 'utf-8');
  const apiConfig: ApiKeyConfig = JSON.parse(configData);

  const baseUrl = apiConfig.testnet
    ? 'https://indexer.v4testnet.dydx.exchange'
    : 'https://indexer.dydx.trade';
  const address = apiConfig.apiKey;
  const subaccountNumber = 0;

  console.log('\n===========================================');
  console.log('dYdX Balance Query');
  console.log('===========================================');
  console.log('Wallet Address:', address);
  console.log('Network:', apiConfig.testnet ? 'Testnet' : 'Mainnet');
  console.log('Base URL:', baseUrl);
  console.log('Subaccount:', subaccountNumber);
  console.log('===========================================\n');

  // Try all possible endpoints
  const endpoints = [
    {
      name: 'All Subaccounts',
      url: `${baseUrl}/v4/addresses/${address}`,
    },
    {
      name: 'Specific Subaccount',
      url: `${baseUrl}/v4/addresses/${address}/subaccountNumber/${subaccountNumber}`,
    },
    {
      name: 'Asset Positions (USDC Balance)',
      url: `${baseUrl}/v4/assetPositions`,
      params: { address, subaccountNumber: subaccountNumber.toString() },
    },
    {
      name: 'Perpetual Positions',
      url: `${baseUrl}/v4/perpetualPositions`,
      params: { address, subaccountNumber: subaccountNumber.toString() },
    },
    {
      name: 'Orders',
      url: `${baseUrl}/v4/orders`,
      params: { address, subaccountNumber: subaccountNumber.toString() },
    },
    {
      name: 'Fills',
      url: `${baseUrl}/v4/fills`,
      params: { address, subaccountNumber: subaccountNumber.toString() },
    },
    {
      name: 'Transfers',
      url: `${baseUrl}/v4/transfers`,
      params: { address, subaccountNumber: subaccountNumber.toString() },
    },
  ];

  for (const endpoint of endpoints) {
    try {
      console.log(`\n📡 Testing: ${endpoint.name}`);
      console.log(`   URL: ${endpoint.url}`);
      if (endpoint.params) {
        console.log(`   Params:`, endpoint.params);
      }

      const response = await axios.get(endpoint.url, { params: endpoint.params });

      console.log(`   ✅ Status: ${response.status}`);
      console.log(`   📦 Data:`, JSON.stringify(response.data, null, 2));

      // Look for balance information
      if (response.data) {
        if (response.data.positions) {
          const usdcPos = response.data.positions.find(
            (p: any) => p.symbol === 'USDC' || p.assetId === '0'
          );
          if (usdcPos) {
            console.log(`   💰 USDC Balance: ${usdcPos.size}`);
          }
        }
        if (response.data.subaccount) {
          console.log(`   📊 Subaccount Data:`, JSON.stringify(response.data.subaccount, null, 2));
        }
      }
    } catch (error: any) {
      console.log(`   ❌ Error: ${error.response?.status || 'Unknown'}`);
      if (error.response) {
        console.log(`   📦 Error Data:`, JSON.stringify(error.response.data, null, 2));
      } else {
        console.log(`   Error Message:`, error.message);
      }
    }
  }

  console.log('\n===========================================');
  console.log('Summary');
  console.log('===========================================');
  console.log('If all endpoints returned 404:');
  console.log('  1. Account exists but has never traded');
  console.log('  2. No USDC deposited or all withdrawn');
  console.log('  3. Check on dYdX web app: https://trade.dydx.exchange/');
  console.log('');
  console.log('To deposit USDC:');
  console.log('  1. Get USDC on the dYdX Chain via bridge');
  console.log('  2. Or transfer from another exchange');
  console.log('===========================================\n');
}

main().catch(console.error);
