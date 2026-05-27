import * as fs from 'fs';
import * as path from 'path';

import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const testMarketPrecision = async () => {
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

  // Find BTC market
  const btcMarket = store.markets.find((m) => m.symbol === 'BTC_USDT_Perp');

  if (btcMarket) {
    console.log('=== BTC_USDT_Perp Market ===');
    console.log('Precision:', btcMarket.precision);
    console.log('Limits:', btcMarket.limits);
    console.log('\nRaw instrument data:');

    // Fetch raw instrument data
    const response = (await (exchange as any).xhr.post('/lite/v1/all_instruments', {})) as { data: { r: any[] } };
    const btcInst = response.data.r.find((i: any) => i.i === 'BTC_USDT_Perp');

    if (btcInst) {
      console.log('  instrument_id (i):', btcInst.i);
      console.log('  base_decimals (bd):', btcInst.bd);
      console.log('  quote_decimals (qd):', btcInst.qd);
      console.log('  tick_size (ts):', btcInst.ts);
      console.log('  min_size (ms):', btcInst.ms);
      console.log('  max_position_size (mp):', btcInst.mp);

      // Calculate precision from min_size
      if (btcInst.ms) {
        const step = parseFloat(btcInst.ms);
        const precision = Math.abs(Math.floor(Math.log10(step)));
        console.log('\n  Calculated amount precision from min_size:', precision, 'decimals');
        console.log('  Min size', btcInst.ms, 'allows increments of', step);
      }
    }
  }

  exchange.dispose();
  console.log('\nTest complete');
};

testMarketPrecision().catch((err) => {
  console.error('Test failed:', err);
  process.exit(1);
});
