#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { BackpackExchange } from './backpack.exchange';
import { DefaultStore } from '../../store/store.base';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found at:', credPath);
  console.log('Please create .apikey.json with your Backpack credentials');
  process.exit(0);
}

async function testBalance() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== Backpack Balance Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet || false);
  console.log('==============================\n');

  const store = new DefaultStore();
  const exchange = new BackpackExchange(
    'backpack',
    {
      key: config.apiKey,
      secret: config.secret,
      testnet: config.testnet || false,
    },
    store
  );

  try {
    console.log('1. Fetching balance...');
    const balance = await exchange.fetchBalance();

    console.log('\n✅ Balance fetched successfully!\n');
    console.log('Total:', `$${balance.total.toFixed(2)}`);
    console.log('Free:', `$${balance.free.toFixed(2)}`);
    console.log('Used:', `$${balance.used.toFixed(2)}`);
    console.log('UPNL:', `$${balance.upnl.toFixed(2)}`);

    if (balance.currencies) {
      const currencies = Object.entries(balance.currencies);
      console.log(`\nCurrencies (${currencies.length}):`);

      for (const [currency, bal] of currencies) {
        const available = (bal as any).available || 0;
        const locked = (bal as any).locked || 0;
        const staked = (bal as any).staked || 0;
        const used = (bal as any).used || 0;
        const total = (bal as any).total || 0;

        if (total > 0) {
          console.log(`\n  ${currency}:`);
          console.log(`    Available: ${available}`);
          console.log(`    Locked:    ${locked}`);
          console.log(`    Staked:    ${staked}`);
          console.log(`    Used:      ${used}`);
          console.log(`    Total:     ${total}`);
        }
      }
    }

    console.log('\n✅ Test completed successfully!\n');
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response) {
      console.error('Status:', err.response.status);
      console.error('Data:', err.response.data);
    }
  } finally {
    exchange.dispose();
  }
}

testBalance().catch(console.error);
