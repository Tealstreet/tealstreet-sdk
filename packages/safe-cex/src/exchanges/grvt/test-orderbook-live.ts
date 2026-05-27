import * as fs from 'fs';
import * as path from 'path';

import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const testOrderbook = async () => {
  // Try to load API key from .apikey.json
  const apikeyPath = path.join(__dirname, '.apikey.json');
  let config: any = {};

  try {
    if (fs.existsSync(apikeyPath)) {
      config = JSON.parse(fs.readFileSync(apikeyPath, 'utf-8'));
      console.log('Loaded API key from .apikey.json');
    }
  } catch {
    console.log('Could not load .apikey.json, checking env vars');
  }

  const apiKey = config.apiKey || process.env.GRVT_API_KEY || '';
  const apiSecret = config.secret || process.env.GRVT_API_SECRET || '';
  const tradingAccountId = config.grvtTradingAccountId || process.env.GRVT_TRADING_ACCOUNT_ID || '';

  if (!apiKey || !apiSecret) {
    console.error('Missing API credentials');
    process.exit(1);
  }

  console.log('Creating exchange instance...');
  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: apiKey,
      secret: apiSecret,
      testnet: true,
      grvtTradingAccountId: tradingAccountId,
    },
    store,
  );

  console.log('Starting exchange...');
  await exchange.start();
  console.log('Exchange started successfully');

  console.log(
    'Markets available:',
    exchange.store.markets.slice(0, 3).map((m) => m.symbol),
  );
  const btcMarket = exchange.store.markets.find((m) => m.symbol === 'BTC_USDT_Perp');
  console.log('BTC_USDT_Perp market:', btcMarket ? `ID: ${btcMarket.id}` : 'NOT FOUND');

  let updateCount = 0;
  console.log('\nSubscribing to BTC_USDT_Perp orderbook...');

  const unsubscribe = exchange.listenOrderBook('BTC_USDT_Perp', (orderbook) => {
    updateCount++;
    console.log(`\n[Update #${updateCount}] Orderbook received at ${new Date().toISOString()}`);
    console.log('  Top 3 Bids:');
    orderbook.bids.slice(0, 3).forEach((bid, i) => {
      console.log(`    ${i + 1}. Price: ${bid.price}, Amount: ${bid.amount}, Total: ${bid.total}`);
    });
    console.log('  Top 3 Asks:');
    orderbook.asks.slice(0, 3).forEach((ask, i) => {
      console.log(`    ${i + 1}. Price: ${ask.price}, Amount: ${ask.amount}, Total: ${ask.total}`);
    });
  });

  console.log('Waiting for orderbook updates (10 seconds)...\n');
  await new Promise((resolve) => setTimeout(resolve, 10000));

  console.log(`\nReceived ${updateCount} orderbook updates total`);
  unsubscribe();
  exchange.dispose();
  console.log('Test complete');
};

testOrderbook().catch((err) => {
  console.error('Test failed:', err);
  process.exit(1);
});
