#!/usr/bin/env ts-node
import { readFileSync } from 'fs';
import { join } from 'path';

import axios from 'axios';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
  dydxAddress?: string;
}

async function main() {
  // Load API credentials
  const configPath = join(__dirname, '..', '.apikey.json');
  const configData = readFileSync(configPath, 'utf-8');
  const apiConfig: ApiKeyConfig = JSON.parse(configData);

  const baseUrl = apiConfig.testnet ? 'https://indexer.v4testnet.dydx.exchange' : 'https://indexer.dydx.trade';
  const address = apiConfig.dydxAddress || apiConfig.apiKey;

  console.log('\n==============================');
  console.log('Direct API Query (No SDK)');
  console.log('==============================');
  console.log('Address:', address);
  console.log('Base URL:', baseUrl);
  console.log('==============================\n');

  // Try to get subaccounts
  console.log('1. GET /v4/addresses/{address}');
  try {
    const response = await axios.get(`${baseUrl}/v4/addresses/${address}`);
    console.log('✅ SUCCESS:');
    console.log(JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.log(`❌ ${error.response?.status || 'ERROR'}: ${error.message}`);
    if (error.response?.data) {
      console.log('Response:', JSON.stringify(error.response.data, null, 2));
    }
  }

  console.log('\n2. GET /v4/addresses/{address}/subaccountNumber/0');
  try {
    const response = await axios.get(`${baseUrl}/v4/addresses/${address}/subaccountNumber/0`);
    console.log('✅ SUCCESS:');
    console.log(JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.log(`❌ ${error.response?.status || 'ERROR'}: ${error.message}`);
  }

  console.log('\n3. GET /v4/perpetualPositions?address={address}&subaccountNumber=0');
  try {
    const response = await axios.get(`${baseUrl}/v4/perpetualPositions`, {
      params: { address, subaccountNumber: '0' },
    });
    console.log('✅ SUCCESS:');
    console.log(JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.log(`❌ ${error.response?.status || 'ERROR'}: ${error.message}`);
  }

  console.log('\n4. GET /v4/assetPositions?address={address}&subaccountNumber=0');
  try {
    const response = await axios.get(`${baseUrl}/v4/assetPositions`, {
      params: { address, subaccountNumber: '0' },
    });
    console.log('✅ SUCCESS:');
    console.log(JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.log(`❌ ${error.response?.status || 'ERROR'}: ${error.message}`);
  }

  // Try all subaccounts
  console.log('\n5. Checking all subaccounts 0-127 for perpetual positions...');
  for (let i = 0; i < 128; i++) {
    try {
      const response = await axios.get(`${baseUrl}/v4/perpetualPositions`, {
        params: { address, subaccountNumber: i.toString() },
      });
      if (response.data?.positions && response.data.positions.length > 0) {
        console.log(`\n✅ FOUND POSITIONS in subaccount ${i}:`);
        console.log(JSON.stringify(response.data, null, 2));
      }
    } catch (_error: any) {
      // Silent - only show found positions
    }

    // Progress indicator
    if (i % 20 === 0) {
      process.stdout.write(`   Checked 0-${i}...\r`);
    }
  }
  console.log('\n   Scan complete.');

  console.log('\n6. Checking /v4/fills for any trade history...');
  try {
    const response = await axios.get(`${baseUrl}/v4/fills`, {
      params: { address },
    });
    console.log('✅ SUCCESS:');
    console.log(JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.log(`❌ ${error.response?.status || 'ERROR'}: ${error.message}`);
  }

  console.log('\n==============================');
  console.log('Direct API query complete');
  console.log('==============================\n');
}

main().catch(console.error);
