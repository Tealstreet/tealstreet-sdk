#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testBalance() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Balance Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('SubAccount ID:', config.subaccountId || '(default)');
  console.log('Signer Address:', config.grvtSignerAddress);
  console.log('===========================\n');

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
    store
  );

  try {
    console.log('1. Authenticating...');
    await exchange.authenticate();
    console.log('✅ Authenticated successfully!\n');

    console.log('2. Fetching balance...');
    const balance = await exchange.fetchBalance();

    console.log('✅ Balance fetched successfully!\n');
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
        const total = (bal as any).total || 0;

        if (total > 0) {
          console.log(`\n  ${currency}:`);
          console.log(`    Available: ${available}`);
          console.log(`    Locked:    ${locked}`);
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
