#!/usr/bin/env tsx
/**
 * Test Paradex ticker updates to verify last price is updating from trades feed
 */

import { readFileSync } from 'fs';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

async function testTickerUpdates() {
  console.log('🔍 Testing Paradex Ticker Updates from Trades Feed...\n');

  // Read credentials from .apikey.json
  const credentialsPath = __dirname + '/.apikey.json';
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

    // Pick a high-volume symbol for testing (BTC, ETH, SOL)
    const testSymbols = ['BTC-USD-PERP', 'ETH-USD-PERP', 'SOL-USD-PERP'];

    console.log('\n📊 Initial ticker state:');
    testSymbols.forEach(symbol => {
      const ticker = store.tickers.find(t => t.symbol === symbol);
      if (ticker) {
        console.log(`  ${symbol}:`);
        console.log(`    - last: ${ticker.last}`);
        console.log(`    - mark: ${ticker.mark}`);
        console.log(`    - bid: ${ticker.bid}`);
        console.log(`    - ask: ${ticker.ask}`);
      } else {
        console.log(`  ${symbol}: NOT FOUND`);
      }
    });

    console.log('\n⏳ Monitoring ticker updates for 30 seconds...');
    console.log('   Watching for changes to last price from trades...\n');

    // Track ticker changes
    const tickerSnapshots: Record<string, any[]> = {};
    testSymbols.forEach(symbol => {
      tickerSnapshots[symbol] = [];
    });

    // Monitor ticker changes every 500ms
    const monitorInterval = setInterval(() => {
      testSymbols.forEach(symbol => {
        const ticker = store.tickers.find(t => t.symbol === symbol);
        if (ticker) {
          const snapshot = {
            time: new Date().toISOString(),
            last: ticker.last,
            mark: ticker.mark,
            bid: ticker.bid,
            ask: ticker.ask,
          };

          // Only log if last price changed
          const prevSnapshots = tickerSnapshots[symbol];
          const prevSnapshot = prevSnapshots[prevSnapshots.length - 1];

          if (!prevSnapshot || prevSnapshot.last !== snapshot.last) {
            console.log(`📈 ${symbol} - last price changed: ${prevSnapshot?.last || 'N/A'} → ${snapshot.last}`);
            console.log(`   mark=${snapshot.mark}, bid=${snapshot.bid}, ask=${snapshot.ask}`);
          }

          tickerSnapshots[symbol].push(snapshot);
        }
      });
    }, 500);

    // Wait for monitoring period
    await new Promise((resolve) => setTimeout(resolve, 30000));
    clearInterval(monitorInterval);

    console.log('\n' + '='.repeat(70));
    console.log('📊 Summary of Ticker Changes:');
    console.log('='.repeat(70));

    testSymbols.forEach(symbol => {
      const snapshots = tickerSnapshots[symbol];
      if (snapshots.length === 0) {
        console.log(`\n${symbol}: No snapshots captured`);
        return;
      }

      const firstSnapshot = snapshots[0];
      const lastSnapshot = snapshots[snapshots.length - 1];

      // Count unique last prices
      const uniqueLastPrices = new Set(snapshots.map(s => s.last)).size;
      const lastPriceChanged = firstSnapshot.last !== lastSnapshot.last;

      console.log(`\n${symbol}:`);
      console.log(`  Total snapshots: ${snapshots.length}`);
      console.log(`  Unique last prices: ${uniqueLastPrices}`);
      console.log(`  First last price: ${firstSnapshot.last}`);
      console.log(`  Final last price: ${lastSnapshot.last}`);
      console.log(`  Last price changed: ${lastPriceChanged ? '✅ YES' : '❌ NO'}`);

      if (lastPriceChanged) {
        console.log(`  Change: ${firstSnapshot.last} → ${lastSnapshot.last} (${((lastSnapshot.last - firstSnapshot.last) / firstSnapshot.last * 100).toFixed(2)}%)`);
      }

      // Show mark vs last
      console.log(`  Mark vs Last: mark=${lastSnapshot.mark}, last=${lastSnapshot.last}`);
      if (lastSnapshot.mark === lastSnapshot.last) {
        console.log(`  ⚠️  Last price equals mark price (using mark as fallback)`);
      }
    });

    console.log('\n' + '='.repeat(70));
    console.log('🔍 Analysis:');
    console.log('='.repeat(70));

    const anyLastPriceChanged = testSymbols.some(symbol => {
      const snapshots = tickerSnapshots[symbol];
      return snapshots.length > 1 && snapshots[0].last !== snapshots[snapshots.length - 1].last;
    });

    if (anyLastPriceChanged) {
      console.log('✅ Last prices are updating from trades feed!');
    } else {
      console.log('⚠️  Last prices are NOT updating from trades feed.');
      console.log('   Possible reasons:');
      console.log('   1. Trades handler not being called (check for "Trades handler called" logs)');
      console.log('   2. No trades occurred during monitoring period');
      console.log('   3. Store.updateTicker not working correctly');
      console.log('   4. Last price being overwritten by BBO with mark price');
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

testTickerUpdates().catch(console.error);
