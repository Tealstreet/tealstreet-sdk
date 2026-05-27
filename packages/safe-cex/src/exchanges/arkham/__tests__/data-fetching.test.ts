/**
 * Test script for Arkham data fetching (markets, tickers, OHLCV, orderbook)
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-data-fetching.ts
 */

import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

// Load API credentials (from parent directory)
const credentialsPath = path.join(__dirname, '..', '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;
const BASE_URL = 'https://arkm.com/api';

console.log('📊 Testing Arkham Data Fetching\n');

function createSignature(
  apiKey: string,
  apiSecret: string,
  method: string,
  path: string,
  body: string = ''
): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000;
  const expiry = timestamp + 5 * 1000000;
  const message = `${apiKey}${expiry}${method}${path}${body}`;

  const decodedSecret = Buffer.from(apiSecret, 'base64');
  const hexSecret = decodedSecret.toString('hex');
  const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
    .update(message)
    .digest('base64');

  return { signature, expiry };
}

/**
 * Test 1: Fetch Markets/Pairs
 */
async function testFetchMarkets() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 1: Fetch Markets/Pairs');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/pairs`);
    const pairs = response.data;

    console.log('✅ Markets fetch successful!');
    console.log(`Total pairs: ${pairs.length}`);

    // Show sample pairs by type
    const spotPairs = pairs.filter((p: any) => p.pairType === 'spot');
    const perpPairs = pairs.filter((p: any) => p.pairType === 'perp');

    console.log(`Spot pairs: ${spotPairs.length}`);
    console.log(`Perpetual pairs: ${perpPairs.length}\n`);

    // Show first spot pair structure
    if (spotPairs.length > 0) {
      console.log('Sample Spot Pair (BTC_USDT):');
      const btcPair = pairs.find((p: any) => p.symbol === 'BTC_USDT');
      console.log(JSON.stringify(btcPair, null, 2));
      console.log();
    }

    // Show first perp pair structure
    if (perpPairs.length > 0) {
      console.log('Sample Perpetual Pair (BTC_PERP):');
      const btcPerp = pairs.find((p: any) => p.symbol === 'BTC_PERP');
      console.log(JSON.stringify(btcPerp, null, 2));
      console.log();
    }

    return { success: true, data: pairs };
  } catch (error: any) {
    console.log('❌ Markets fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 2: Fetch All Tickers
 */
async function testFetchTickers() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 2: Fetch All Tickers');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/tickers`);
    const tickers = response.data;

    console.log('✅ Tickers fetch successful!');
    console.log(`Total tickers: ${tickers.length}\n`);

    // Show sample ticker
    const btcTicker = tickers.find((t: any) => t.symbol === 'BTC_USDT');
    if (btcTicker) {
      console.log('Sample Ticker (BTC_USDT):');
      console.log(JSON.stringify(btcTicker, null, 2));
      console.log();
    }

    return { success: true, data: tickers };
  } catch (error: any) {
    console.log('❌ Tickers fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 3: Fetch Single Ticker
 */
async function testFetchSingleTicker() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 3: Fetch Single Ticker (BTC_USDT)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/ticker`, {
      params: { symbol: 'BTC_USDT' },
    });

    console.log('✅ Single ticker fetch successful!');
    console.log(JSON.stringify(response.data, null, 2));
    console.log();
    return { success: true, data: response.data };
  } catch (error: any) {
    console.log('❌ Single ticker fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 4: Fetch OHLCV/Candles
 */
async function testFetchOHLCV() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 4: Fetch OHLCV/Candles (BTC_USDT, 1h)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const endTime = Date.now() * 1000; // Convert to microseconds
    const startTime = endTime - 24 * 60 * 60 * 1000000; // Last 24 hours in microseconds

    const response = await axios.get(`${BASE_URL}/public/candles`, {
      params: {
        symbol: 'BTC_USDT',
        duration: '1h', // Required parameter, not 'interval'
        start: startTime,
        end: endTime,
      },
    });

    const candles = Array.isArray(response.data) ? response.data : response.data.candles || [];

    console.log('✅ OHLCV fetch successful!');
    console.log(`Received ${candles.length} candles\n`);

    if (candles.length > 0) {
      console.log('First candle:');
      console.log(JSON.stringify(candles[0], null, 2));
      console.log('\nLast candle:');
      console.log(JSON.stringify(candles[candles.length - 1], null, 2));
      console.log();
    }

    return { success: true, data: candles };
  } catch (error: any) {
    console.log('❌ OHLCV fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 5: Fetch Orderbook
 */
async function testFetchOrderbook() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 5: Fetch Orderbook (BTC_USDT)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/book`, {
      params: {
        symbol: 'BTC_USDT',
        depth: 20,
      },
    });

    const orderbook = response.data;

    console.log('✅ Orderbook fetch successful!');
    console.log(`Bids: ${orderbook.bids?.length || 0}`);
    console.log(`Asks: ${orderbook.asks?.length || 0}\n`);

    if (orderbook.bids && orderbook.bids.length > 0) {
      console.log('Top 5 Bids:');
      orderbook.bids.slice(0, 5).forEach((bid: any, i: number) => {
        console.log(`  ${i + 1}. Price: ${bid.price || bid[0]}, Size: ${bid.size || bid[1]}`);
      });
      console.log();
    }

    if (orderbook.asks && orderbook.asks.length > 0) {
      console.log('Top 5 Asks:');
      orderbook.asks.slice(0, 5).forEach((ask: any, i: number) => {
        console.log(`  ${i + 1}. Price: ${ask.price || ask[0]}, Size: ${ask.size || ask[1]}`);
      });
      console.log();
    }

    return { success: true, data: orderbook };
  } catch (error: any) {
    console.log('❌ Orderbook fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 6: Fetch Level-1 Orderbook
 */
async function testFetchLevel1Orderbook() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 6: Fetch Level-1 Orderbook (BTC_USDT)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/level-one-book`, {
      params: { symbol: 'BTC_USDT' },
    });

    console.log('✅ Level-1 orderbook fetch successful!');
    console.log(JSON.stringify(response.data, null, 2));
    console.log();
    return { success: true, data: response.data };
  } catch (error: any) {
    console.log('❌ Level-1 orderbook fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Test 7: Fetch Public Trades
 */
async function testFetchPublicTrades() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('Test 7: Fetch Public Trades (BTC_USDT)');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  try {
    const response = await axios.get(`${BASE_URL}/public/trades`, {
      params: {
        symbol: 'BTC_USDT',
        limit: 10,
      },
    });

    const trades = Array.isArray(response.data) ? response.data : response.data.trades || [];

    console.log('✅ Public trades fetch successful!');
    console.log(`Received ${trades.length} trades\n`);

    if (trades.length > 0) {
      console.log('Recent trades:');
      trades.slice(0, 5).forEach((trade: any, i: number) => {
        console.log(`  ${i + 1}. Price: ${trade.price}, Size: ${trade.size || trade.amount}, Side: ${trade.side}`);
      });
      console.log();
    }

    return { success: true, data: trades };
  } catch (error: any) {
    console.log('❌ Public trades fetch failed!');
    console.log('Status:', error.response?.status);
    console.log('Error:', error.response?.data || error.message, '\n');
    return { success: false };
  }
}

/**
 * Run all tests
 */
async function runAllTests() {
  console.log('🚀 Starting Arkham Data Fetching Tests\n');

  const results = {
    markets: await testFetchMarkets(),
    tickers: await testFetchTickers(),
    singleTicker: await testFetchSingleTicker(),
    ohlcv: await testFetchOHLCV(),
    orderbook: await testFetchOrderbook(),
    level1Orderbook: await testFetchLevel1Orderbook(),
    publicTrades: await testFetchPublicTrades(),
  };

  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📊 Test Results Summary');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  Object.entries(results).forEach(([test, result]) => {
    console.log(`${result.success ? '✅' : '❌'} ${test}`);
  });

  const totalTests = Object.keys(results).length;
  const passedTests = Object.values(results).filter((r) => r.success).length;

  console.log(`\n${passedTests}/${totalTests} tests passed\n`);

  if (passedTests === totalTests) {
    console.log('🎉 All data fetching tests passed!\n');
  } else {
    console.log('⚠️  Some tests failed. Please review the errors above.\n');
  }

  return results;
}

// Run tests
runAllTests().catch(console.error);
