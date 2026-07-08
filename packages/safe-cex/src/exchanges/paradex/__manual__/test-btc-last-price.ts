#!/usr/bin/env tsx
/**
 * Test BTC-USD-PERP last price updates for 30+ seconds to verify:
 * 1. Trades update last price
 * 2. BBO doesn't overwrite with 0
 * 3. Last price persists and updates correctly
 */

import { readFileSync } from 'fs';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

async function testBTCLastPrice() {
  console.log('🔍 Testing BTC-USD-PERP Last Price Updates (45 seconds)...\n');

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

    const symbol = 'BTC-USD-PERP';

    console.log('\n📊 Initial State:');
    const initialTicker = store.tickers.find(t => t.symbol === symbol);
    if (initialTicker) {
      console.log(`  Last: ${initialTicker.last}`);
      console.log(`  Mark: ${initialTicker.mark}`);
      console.log(`  Bid: ${initialTicker.bid}`);
      console.log(`  Ask: ${initialTicker.ask}`);
    } else {
      console.log(`  ❌ Ticker not found!`);
      process.exit(1);
    }

    console.log('\n⏳ Monitoring for 45 seconds...\n');
    console.log('Legend:');
    console.log('  📈 = Last price changed');
    console.log('  🔄 = BBO update received');
    console.log('  ✅ = Trade received\n');

    let lastSeenPrice = initialTicker.last;
    let priceChanges = 0;
    let tradeCount = 0;
    let bboCount = 0;
    let bboWithZeroCount = 0;
    let lastPriceWentToZero = false;

    const startTime = Date.now();
    const monitorInterval = setInterval(() => {
      const ticker = store.tickers.find(t => t.symbol === symbol);
      if (!ticker) return;

      const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

      // Check if last price changed
      if (ticker.last !== lastSeenPrice) {
        console.log(`[${elapsed}s] 📈 Last price changed: ${lastSeenPrice} → ${ticker.last}`);

        if (ticker.last === 0) {
          console.log(`       ⚠️  WARNING: Last price went to ZERO!`);
          lastPriceWentToZero = true;
        }

        lastSeenPrice = ticker.last;
        priceChanges++;
      }
    }, 100); // Check every 100ms

    // Also intercept logs to count trades and BBOs
    const originalLog = (exchange as any).log;
    (exchange as any).log = function(message: string, severity: any) {
      if (message.includes('BTC-USD-PERP')) {
        if (message.includes('✅ Updating last price from trade')) {
          tradeCount++;
          const priceMatch = message.match(/= ([\d.]+)/);
          if (priceMatch) {
            console.log(`[${((Date.now() - startTime) / 1000).toFixed(1)}s] ✅ Trade: price=${priceMatch[1]}`);
          }
        } else if (message.includes('BBO update for BTC-USD-PERP')) {
          bboCount++;
          const bidMatch = message.match(/bid=([\d.]+)/);
          const askMatch = message.match(/ask=([\d.]+)/);
          const markMatch = message.match(/mark=([\d.]+)/);
          const lastMatch = message.match(/last=([\d.]+)/);

          if (bidMatch && askMatch && markMatch) {
            const bid = parseFloat(bidMatch[1]);
            const ask = parseFloat(askMatch[1]);
            const mark = parseFloat(markMatch[1]);
            const last = lastMatch ? parseFloat(lastMatch[1]) : undefined;

            if (bid === 0 && ask === 0 && mark === 0) {
              bboWithZeroCount++;
              console.log(`[${((Date.now() - startTime) / 1000).toFixed(1)}s] 🔄 BBO: ALL ZEROS (bid=0, ask=0, mark=0)${last !== undefined ? ` last=${last}` : ' (no last)'}`);
            } else {
              console.log(`[${((Date.now() - startTime) / 1000).toFixed(1)}s] 🔄 BBO: bid=${bid}, ask=${ask}, mark=${mark}${last !== undefined ? `, last=${last}` : ''}`);
            }
          }
        }
      }
      originalLog.call(this, message, severity);
    };

    // Wait for monitoring period
    await new Promise((resolve) => setTimeout(resolve, 45000));
    clearInterval(monitorInterval);

    console.log('\n' + '='.repeat(70));
    console.log('📊 Summary:');
    console.log('='.repeat(70));

    const finalTicker = store.tickers.find(t => t.symbol === symbol);

    console.log(`\nInitial last price: ${initialTicker.last}`);
    console.log(`Final last price:   ${finalTicker?.last || 'NOT FOUND'}`);
    console.log(`Price changed:      ${priceChanges} times`);
    console.log(``);
    console.log(`Trade updates:      ${tradeCount}`);
    console.log(`BBO updates:        ${bboCount}`);
    console.log(`BBO with zeros:     ${bboWithZeroCount}`);
    console.log(``);

    console.log('='.repeat(70));
    console.log('🔍 Analysis:');
    console.log('='.repeat(70));

    if (lastPriceWentToZero) {
      console.log('❌ FAIL: Last price went to ZERO during monitoring');
      console.log('   This means BBO is still overwriting with 0');
    } else if (priceChanges === 0 && tradeCount === 0) {
      console.log('⚠️  No price changes and no trades during monitoring period');
      console.log('   Market might be very quiet, try running longer or on different symbol');
    } else if (priceChanges > 0 && !lastPriceWentToZero) {
      console.log('✅ SUCCESS: Last price updated and never went to zero');
      console.log('   Trades are updating last price correctly');
      console.log('   BBO is not overwriting with 0');
    } else if (priceChanges === 0 && tradeCount > 0) {
      console.log('⚠️  Trades received but last price not changing');
      console.log('   Trades might be updating but BBO overwriting immediately');
    }

    if (bboWithZeroCount > 0) {
      console.log(`\n⚠️  Received ${bboWithZeroCount} BBO updates with all zeros`);
      if (!lastPriceWentToZero) {
        console.log('   But last price was preserved (FIX IS WORKING!)');
      }
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

testBTCLastPrice().catch(console.error);
