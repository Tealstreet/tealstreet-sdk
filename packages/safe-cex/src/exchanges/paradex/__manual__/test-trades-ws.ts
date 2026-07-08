#!/usr/bin/env tsx
/**
 * Test Paradex trades WebSocket feed to see if we can use it for last price updates
 */

import { readFileSync } from 'fs';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import type { ExchangeOptions } from '../../../types';

async function testTradesFeed() {
  console.log('🔍 Testing Paradex Trades WebSocket Feed...\n');

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

    console.log('\n📊 Available markets:');
    const markets = store.markets.slice(0, 5); // Just first 5 for testing
    markets.forEach((m) => console.log(`  - ${m.symbol}`));

    console.log('\n🔌 Subscribing to trades feed for first market...');
    const testSymbol = markets[0].symbol;
    console.log(`   Symbol: ${testSymbol}`);

    // Access the public WebSocket
    const publicWs = (exchange as any).publicWebsocket;

    // Manually subscribe to trades channel
    const tradesChannel = `trades.${testSymbol}`;
    const subscribeMsg = {
      id: Date.now(),
      jsonrpc: '2.0',
      method: 'subscribe',
      params: { channel: tradesChannel },
    };

    console.log(`   Sending subscription: ${JSON.stringify(subscribeMsg)}`);
    publicWs.send(JSON.stringify(subscribeMsg));

    console.log('\n⏳ Listening for trades updates (30 seconds)...');
    console.log('   Press Ctrl+C to exit early\n');

    // Intercept WebSocket messages to see trades
    const originalOnMessage = publicWs.onMessage.bind(publicWs);
    let tradesReceived = 0;
    let lastTradePrice = null;

    publicWs.onMessage = (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data);

        // Check if this is a trades update
        if (data.method === 'subscription' && data.params?.channel?.startsWith('trades.')) {
          tradesReceived++;
          const tradesData = data.params.data;

          console.log(`\n📈 Trade Update #${tradesReceived}:`);
          console.log('   Raw data:', JSON.stringify(data.params, null, 2));

          // Try to extract trade info
          if (tradesData) {
            console.log('\n   Parsed trade data:');
            console.log(`   - Market: ${tradesData.market || 'N/A'}`);
            console.log(`   - Price: ${tradesData.price || 'N/A'}`);
            console.log(`   - Size: ${tradesData.size || tradesData.amount || 'N/A'}`);
            console.log(`   - Side: ${tradesData.side || 'N/A'}`);
            console.log(`   - Timestamp: ${tradesData.timestamp || tradesData.created_at || 'N/A'}`);

            if (tradesData.price) {
              lastTradePrice = tradesData.price;
              console.log(`\n   ✅ Last trade price: ${lastTradePrice}`);
            }
          }
        }
      } catch (_err) {
        // Ignore parse errors, just pass to original handler
      }

      // Call original handler
      originalOnMessage(event);
    };

    // Wait for trades
    await new Promise((resolve) => setTimeout(resolve, 30000));

    console.log('\n' + '='.repeat(70));
    console.log('📊 Summary:');
    console.log('='.repeat(70));
    console.log(`Total trades received: ${tradesReceived}`);
    console.log(`Last trade price: ${lastTradePrice || 'No trades received'}`);

    if (tradesReceived > 0) {
      console.log('\n✅ Trades feed is working! Can use for last price updates.');
    } else {
      console.log('\n⚠️  No trades received. Market might be inactive or subscription failed.');
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

testTradesFeed().catch(console.error);
