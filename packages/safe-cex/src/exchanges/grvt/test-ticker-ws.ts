import * as fs from 'fs';
import * as path from 'path';

import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const testTickerWS = async () => {
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

  console.log(`Markets loaded: ${store.markets.length}`);
  console.log(`Initial tickers: ${store.tickers.length}\n`);

  // Wait for WebSocket to connect and subscribe to tickers
  await new Promise((resolve) => setTimeout(resolve, 2000));

  console.log('=== Waiting for ticker updates (30 seconds) ===\n');

  // Track ticker updates
  let updateCount = 0;
  const tickerUpdates: Map<string, number> = new Map();

  const interval = setInterval(() => {
    const currentTickerCount = store.tickers.length;
    if (currentTickerCount !== updateCount) {
      updateCount = currentTickerCount;
      console.log(`Tickers updated: ${currentTickerCount} total`);

      // Show sample of tickers with real data
      const tickersWithData = store.tickers.filter((t) => t.last > 0);
      console.log(`Tickers with real data: ${tickersWithData.length}`);

      if (tickersWithData.length > 0) {
        console.log('\nSample tickers:');
        tickersWithData.slice(0, 5).forEach((t) => {
          const updates = tickerUpdates.get(t.symbol) || 0;
          tickerUpdates.set(t.symbol, updates + 1);
          console.log(`  ${t.symbol}:`);
          console.log(`    Last: ${t.last}, Bid: ${t.bid}, Ask: ${t.ask}`);
          console.log(`    Mark: ${t.mark}, Index: ${t.index}`);
          console.log(`    24h Change: ${t.percentage.toFixed(2)}%`);
          console.log(`    Volume: ${t.volume.toFixed(2)}`);
          console.log(`    Funding Rate: ${t.fundingRate}`);
          console.log(`    Open Interest: ${t.openInterest}`);
        });
        console.log();
      }
    }
  }, 1000);

  // Wait 30 seconds
  await new Promise((resolve) => setTimeout(resolve, 30000));
  clearInterval(interval);

  console.log('\n=== Final Results ===');
  console.log(`Total tickers: ${store.tickers.length}`);
  const tickersWithData = store.tickers.filter((t) => t.last > 0);
  console.log(`Tickers with real data: ${tickersWithData.length}`);
  console.log(`Tickers receiving updates: ${tickerUpdates.size}`);

  if (tickersWithData.length > 0) {
    console.log('\nTop 10 tickers by update count:');
    const sorted = Array.from(tickerUpdates.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 10);
    sorted.forEach(([symbol, count]) => {
      const ticker = store.tickers.find((t) => t.symbol === symbol);
      console.log(`  ${symbol}: ${count} updates, last price: ${ticker?.last}`);
    });
  }

  exchange.dispose();
  console.log('\nTest complete');
};

testTickerWS().catch((err) => {
  console.error('Test failed:', err);
  process.exit(1);
});
