import * as fs from 'fs';
import * as path from 'path';

import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const testFetchOHLCV = async () => {
  const apikeyPath = path.join(__dirname, '.apikey.json');
  let config: any = {};

  try {
    if (fs.existsSync(apikeyPath)) {
      config = JSON.parse(fs.readFileSync(apikeyPath, 'utf-8'));
    }
  } catch {
    console.log('Could not load .apikey.json');
  }

  const apiKey = config.apiKey || process.env.GRVT_API_KEY || '';
  const apiSecret = config.secret || process.env.GRVT_API_SECRET || '';

  if (!apiKey || !apiSecret) {
    console.error('Missing API credentials');
    process.exit(1);
  }

  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: apiKey,
      secret: apiSecret,
      testnet: true,
    },
    store,
  );

  console.log('Starting exchange...');
  await exchange.start();
  console.log('Exchange started\n');

  const symbol = 'BTC_USDT_Perp';

  // Test 1: Fetch with limit only
  console.log('=== Test 1: fetchOHLCV with limit=5 ===');
  try {
    const candles1 = await exchange.fetchOHLCV({
      symbol,
      interval: '1m',
      limit: 5,
    });
    console.log(`✅ Received ${candles1.length} candles`);
    if (candles1.length > 0) {
      console.log('First:', new Date(candles1[0].timestamp).toISOString(), candles1[0]);
      console.log(
        'Last:',
        new Date(candles1[candles1.length - 1].timestamp).toISOString(),
        candles1[candles1.length - 1],
      );
    }
  } catch (err: any) {
    console.error('❌ Error:', err.message);
    if (err.response?.data) {
      console.error('Response:', JSON.stringify(err.response.data, null, 2));
    }
  }

  // Test 2: Fetch with from/to
  console.log('\n=== Test 2: fetchOHLCV with from/to (last hour) ===');
  try {
    const now = Date.now();
    const oneHourAgo = now - 60 * 60 * 1000;

    const candles2 = await exchange.fetchOHLCV({
      symbol,
      interval: '1m',
      from: oneHourAgo,
      to: now,
    });
    console.log(`✅ Received ${candles2.length} candles`);
    if (candles2.length > 0) {
      console.log('First:', new Date(candles2[0].timestamp).toISOString());
      console.log('Last:', new Date(candles2[candles2.length - 1].timestamp).toISOString());
    }
  } catch (err: any) {
    console.error('❌ Error:', err.message);
    if (err.response?.data) {
      console.error('Response:', JSON.stringify(err.response.data, null, 2));
    }
  }

  // Test 3: Fetch with from only
  console.log('\n=== Test 3: fetchOHLCV with from only (last 10 min) ===');
  try {
    const tenMinAgo = Date.now() - 10 * 60 * 1000;

    const candles3 = await exchange.fetchOHLCV({
      symbol,
      interval: '1m',
      from: tenMinAgo,
      limit: 20,
    });
    console.log(`✅ Received ${candles3.length} candles`);
    if (candles3.length > 0) {
      console.log('First:', new Date(candles3[0].timestamp).toISOString());
      console.log('Last:', new Date(candles3[candles3.length - 1].timestamp).toISOString());
    }
  } catch (err: any) {
    console.error('❌ Error:', err.message);
    if (err.response?.data) {
      console.error('Response:', JSON.stringify(err.response.data, null, 2));
    }
  }

  exchange.dispose();
  console.log('\nTest complete');
};

testFetchOHLCV().catch((err) => {
  console.error('Test failed:', err);
  process.exit(1);
});
