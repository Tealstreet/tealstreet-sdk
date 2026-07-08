#!/usr/bin/env tsx
/**
 * Dump raw Paradex balance response to see what the API actually returns
 */

import { readFileSync } from 'fs';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

async function dumpBalance() {
  console.log('🔍 Dumping Paradex Balance Data...\n');

  // Read credentials from .apikey.json
  const credentialsPath = './src/exchanges/paradex/.apikey.json';
  let credentials: any;

  try {
    const credentialsData = readFileSync(credentialsPath, 'utf-8');
    credentials = JSON.parse(credentialsData);
    console.log('✅ Loaded credentials from .apikey.json');
  } catch (error) {
    console.error('❌ Error reading .apikey.json:', error);
    process.exit(1);
  }

  const options: ExchangeOptions = {
    key: credentials.apiKey,
    secret: '',
    testnet: credentials.testnet || false,
    paradexAddress: credentials.paradexAddress,
    subkeyAddress: credentials.subkeyAddress,
  };

  const store = new DefaultStore();
  const exchange = new ParadexExchange('paradex', options, store);

  try {
    console.log('🚀 Starting exchange...');
    await exchange.start();

    console.log('⏳ Waiting for initialization...');
    await new Promise((resolve) => setTimeout(resolve, 3000));

    console.log('\n📡 Fetching balance from API...');
    const paradexExchange = exchange as any;

    // Get raw API response
    console.log('   Calling: GET /account');
    const rawResponse = await paradexExchange.xhr.get('/account');

    console.log('\n📋 Raw API Response:');
    console.log('='.repeat(70));
    console.log(JSON.stringify(rawResponse.data, null, 2));
    console.log('='.repeat(70));

    console.log('\n🔑 Key Fields:');
    const data = rawResponse.data;
    console.log('  collateral:       ', data.collateral);
    console.log('  equity:           ', data.equity);
    console.log('  free_collateral:  ', data.free_collateral);
    console.log('  position_margin:  ', data.position_margin);
    console.log('  buying_power:     ', data.buying_power);
    console.log('  max_withdrawable: ', data.max_withdrawable);

    console.log('\n📊 Parsed Balance (using fetchBalance):');
    const balance = await exchange.fetchBalance();
    console.log('  total: ', balance.total, ' ← from collateral');
    console.log('  free:  ', balance.free, ' ← from free_collateral');
    console.log('  used:  ', balance.used, ' ← from position_margin');
    console.log('  upnl:  ', balance.upnl, ' ← calculated later');

    console.log('\n🧮 Calculations:');
    const collateral = parseFloat(data.collateral || '0');
    const equity = parseFloat(data.equity || '0');
    const freeCollateral = parseFloat(data.free_collateral || '0');
    const positionMargin = parseFloat(data.position_margin || '0');

    console.log('  Implied UPNL: ', equity - collateral, ' (equity - collateral)');
    console.log('  Sum free+used:', freeCollateral + positionMargin, ' (free_collateral + position_margin)');

    if (Math.abs(collateral - (freeCollateral + positionMargin)) < 0.01) {
      console.log('  ✓ collateral ≈ free + used (valid)');
    } else {
      console.log('  ⚠️  collateral != free + used (check this!)');
    }

    console.log('\n✅ Test complete!');
  } catch (error: any) {
    console.error('\n❌ Error:', error.message);
    if (error.response) {
      console.error('Response status:', error.response.status);
      console.error('Response data:', JSON.stringify(error.response.data, null, 2));
    }
    console.error('Stack:', error.stack);
  } finally {
    await exchange.dispose();
  }
}

dumpBalance().catch(console.error);
