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

async function testPositions() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Positions Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Testnet:', config.testnet);
  console.log('Trading Account ID:', config.grvtTradingAccountId || '(not set)');
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

    console.log('2. Fetching positions...');
    const positions = await exchange.fetchPositions();

    console.log(`✅ Fetched ${positions.length} position(s)\n`);

    if (positions.length === 0) {
      console.log('No positions found.');
    } else {
      for (const pos of positions) {
        console.log(`Position: ${pos.symbol}`);
        console.log(`  Side: ${pos.side}`);
        console.log(`  Contracts: ${pos.contracts}`);
        console.log(`  Entry Price: $${pos.entryPrice}`);
        console.log(`  Notional: $${pos.notional.toFixed(2)}`);
        console.log(`  Leverage: ${pos.leverage}x`);
        console.log(`  Unrealized PnL: $${pos.unrealizedPnl.toFixed(2)}`);
        console.log(`  Liquidation Price: $${pos.liquidationPrice}`);
        console.log('');
      }
    }

    console.log('✅ Test completed successfully!\n');
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response) {
      console.error('Status:', err.response.status);
      console.error('Data:', err.response.data);
    }
    if (err.stack) {
      console.error('Stack:', err.stack);
    }
  } finally {
    exchange.dispose();
  }
}

testPositions().catch(console.error);
