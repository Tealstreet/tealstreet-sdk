import * as fs from 'fs';
import * as path from 'path';

import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const testOHLCV = async () => {
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
  console.log('Exchange started successfully\n');

  const symbol = 'BTC_USDT_Perp';
  const interval = '1m';

  // Test 1: fetchOHLCV
  console.log('=== Test 1: fetchOHLCV ===');
  console.log(`Fetching last 10 ${interval} candles for ${symbol}...\n`);

  try {
    const candles = await exchange.fetchOHLCV({
      symbol,
      interval,
      limit: 10,
    });

    console.log(`✅ Received ${candles.length} candles`);
    if (candles.length > 0) {
      console.log('\nFirst candle:');
      console.log('  Timestamp:', new Date(candles[0].timestamp).toISOString());
      console.log('  Open:', candles[0].open);
      console.log('  High:', candles[0].high);
      console.log('  Low:', candles[0].low);
      console.log('  Close:', candles[0].close);
      console.log('  Volume:', candles[0].volume);

      console.log('\nLast candle:');
      const last = candles[candles.length - 1];
      console.log('  Timestamp:', new Date(last.timestamp).toISOString());
      console.log('  Open:', last.open);
      console.log('  High:', last.high);
      console.log('  Low:', last.low);
      console.log('  Close:', last.close);
      console.log('  Volume:', last.volume);
    }
  } catch (err: any) {
    console.error('❌ fetchOHLCV failed:', err.message);
    if (err.response?.data) {
      console.error('Response data:', JSON.stringify(err.response.data, null, 2));
    }
  }

  // Test 2: listenOHLCV
  console.log('\n\n=== Test 2: listenOHLCV (WebSocket) ===');
  console.log(`Subscribing to ${interval} candles for ${symbol}...\n`);

  let updateCount = 0;
  const unsubscribe = exchange.listenOHLCV({ symbol, interval }, (candle) => {
    updateCount++;
    console.log(`\n[Update #${updateCount}] Candle received at ${new Date().toISOString()}`);
    console.log('  Timestamp:', new Date(candle.timestamp).toISOString());
    console.log('  Open:', candle.open);
    console.log('  High:', candle.high);
    console.log('  Low:', candle.low);
    console.log('  Close:', candle.close);
    console.log('  Volume:', candle.volume);
  });

  console.log('Waiting for candle updates (60 seconds)...\n');
  await new Promise((resolve) => setTimeout(resolve, 60000));

  console.log(`\n✅ Received ${updateCount} candle updates total`);
  unsubscribe();
  exchange.dispose();
  console.log('\nTest complete');
};

testOHLCV().catch((err) => {
  console.error('Test failed:', err);
  process.exit(1);
});
