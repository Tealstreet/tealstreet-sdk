import * as fs from 'fs';
import * as path from 'path';

import { DefaultStore } from '../../store/store.base';
import { BybitExchange } from '../bybit/bybit.exchange';
import { GrvtExchange } from './grvt.exchange';

const compareOHLCV = async () => {
  console.log('=== Comparing GRVT vs Bybit OHLCV ===\n');

  // Load GRVT config
  const grvtConfigPath = path.join(__dirname, '.apikey.json');
  let grvtConfig: any = {};
  try {
    if (fs.existsSync(grvtConfigPath)) {
      grvtConfig = JSON.parse(fs.readFileSync(grvtConfigPath, 'utf-8'));
    }
  } catch {
    console.log('Could not load GRVT .apikey.json');
  }

  // Initialize exchanges
  const grvtStore = new DefaultStore();
  const grvt = new GrvtExchange(
    'grvt',
    {
      key: grvtConfig.apiKey || '',
      secret: grvtConfig.secret || '',
      testnet: true,
    },
    grvtStore,
  );

  const bybitStore = new DefaultStore();
  const bybit = new BybitExchange(
    'bybit',
    {
      key: '',
      secret: '',
      testnet: false,
    },
    bybitStore,
  );

  console.log('Starting exchanges...');
  await Promise.all([grvt.start(), bybit.start()]);
  console.log('Exchanges started\n');

  // Fetch same data from both
  const limit = 5;
  const interval = '1m';

  console.log(`Fetching last ${limit} ${interval} candles from both exchanges...\n`);

  const [grvtCandles, bybitCandles] = await Promise.all([
    grvt.fetchOHLCV({ symbol: 'BTC_USDT_Perp', interval, limit }),
    bybit.fetchOHLCV({ symbol: 'BTCUSDT', interval, limit }),
  ]);

  console.log('=== GRVT Results ===');
  console.log(`Count: ${grvtCandles.length}`);
  console.log('Candles:');
  grvtCandles.forEach((c, i) => {
    console.log(
      `  [${i}] ${new Date(c.timestamp).toISOString()} | O:${c.open} H:${c.high} L:${c.low} C:${c.close} V:${c.volume}`,
    );
  });

  console.log('\n=== Bybit Results ===');
  console.log(`Count: ${bybitCandles.length}`);
  console.log('Candles:');
  bybitCandles.forEach((c, i) => {
    console.log(
      `  [${i}] ${new Date(c.timestamp).toISOString()} | O:${c.open} H:${c.high} L:${c.low} C:${c.close} V:${c.volume}`,
    );
  });

  console.log('\n=== Analysis ===');
  console.log(`GRVT candle count: ${grvtCandles.length}`);
  console.log(`Bybit candle count: ${bybitCandles.length}`);

  if (grvtCandles.length > 0 && bybitCandles.length > 0) {
    const grvtFirst = grvtCandles[0];
    const grvtLast = grvtCandles[grvtCandles.length - 1];
    const bybitFirst = bybitCandles[0];
    const bybitLast = bybitCandles[bybitCandles.length - 1];

    console.log('\nGRVT:');
    console.log(`  First candle: ${new Date(grvtFirst.timestamp).toISOString()}`);
    console.log(`  Last candle:  ${new Date(grvtLast.timestamp).toISOString()}`);
    console.log(
      `  Order: ${grvtFirst.timestamp < grvtLast.timestamp ? 'Chronological (oldest→newest) ✅' : 'Reverse chronological (newest→oldest) ❌'}`,
    );

    console.log('\nBybit:');
    console.log(`  First candle: ${new Date(bybitFirst.timestamp).toISOString()}`);
    console.log(`  Last candle:  ${new Date(bybitLast.timestamp).toISOString()}`);
    console.log(
      `  Order: ${bybitFirst.timestamp < bybitLast.timestamp ? 'Chronological (oldest→newest) ✅' : 'Reverse chronological (newest→oldest) ❌'}`,
    );

    console.log('\nField comparison (first candle):');
    console.log('GRVT:');
    console.log('  timestamp:', grvtFirst.timestamp, typeof grvtFirst.timestamp);
    console.log('  open:', grvtFirst.open, typeof grvtFirst.open);
    console.log('  high:', grvtFirst.high, typeof grvtFirst.high);
    console.log('  low:', grvtFirst.low, typeof grvtFirst.low);
    console.log('  close:', grvtFirst.close, typeof grvtFirst.close);
    console.log('  volume:', grvtFirst.volume, typeof grvtFirst.volume);

    console.log('\nBybit:');
    console.log('  timestamp:', bybitFirst.timestamp, typeof bybitFirst.timestamp);
    console.log('  open:', bybitFirst.open, typeof bybitFirst.open);
    console.log('  high:', bybitFirst.high, typeof bybitFirst.high);
    console.log('  low:', bybitFirst.low, typeof bybitFirst.low);
    console.log('  close:', bybitFirst.close, typeof bybitFirst.close);
    console.log('  volume:', bybitFirst.volume, typeof bybitFirst.volume);
  }

  grvt.dispose();
  bybit.dispose();
  console.log('\nComparison complete');
};

compareOHLCV().catch((err) => {
  console.error('Comparison failed:', err);
  process.exit(1);
});
