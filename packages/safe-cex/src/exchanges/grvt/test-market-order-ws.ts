#!/usr/bin/env npx tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { OrderSide, OrderType } from '../../types';
import { DefaultStore } from '../../store/store.base';
import { GrvtExchange } from './grvt.exchange';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

const config = JSON.parse(readFileSync(credPath, 'utf-8'));

console.log('=== GRVT Market Order WebSocket Test ===');
console.log('Will place 0.001 BTC market buy order');
console.log('========================================\n');

async function main() {
  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: config.apiKey,
      secret: config.secret,
      testnet: config.testnet || false,
      subaccount: config.grvtTradingAccountId || '',
    },
    store,
  );

  // Track WebSocket messages
  let wsMessages = 0;
  const originalOnMessage = (exchange as any).wsPrivate?.onMessage;
  if (originalOnMessage) {
    (exchange as any).wsPrivate.onMessage = function(event: MessageEvent) {
      wsMessages++;
      console.log(`📨 WebSocket message #${wsMessages}`);
      return originalOnMessage.call(this, event);
    };
  }

  try {
    // Start exchange (authenticates and connects WebSocket)
    console.log('1. Starting exchange...');
    await exchange.start();
    console.log('✅ Exchange started\n');

    // Wait for WebSocket to be ready
    console.log('2. Waiting for WebSocket connection (3 seconds)...');
    await new Promise(resolve => setTimeout(resolve, 3000));
    console.log('✅ WebSocket ready\n');

    // Get current BTC position before order
    const btcPositionBefore = store.positions.find(p => p.symbol === 'BTC_USDT_Perp');
    const contractsBefore = btcPositionBefore?.contracts || 0;
    console.log('3. Current BTC position:', btcPositionBefore ? {
      side: btcPositionBefore.side,
      contracts: btcPositionBefore.contracts,
      entryPrice: btcPositionBefore.entryPrice,
    } : 'None');
    console.log();

    // Place market buy order
    console.log('4. Placing 0.001 BTC market buy order...');
    const orderIds = await exchange.placeOrder({
      symbol: 'BTC_USDT_Perp',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 0.001,
    });
    console.log('✅ Order placed:', {
      orderIds,
    });
    console.log();

    // Wait for WebSocket updates
    console.log('5. Waiting for WebSocket updates (15 seconds)...');
    const checkInterval = setInterval(() => {
      const btcPosition = store.positions.find(p => p.symbol === 'BTC_USDT_Perp');
      const contractsNow = btcPosition?.contracts || 0;
      if (contractsNow !== contractsBefore) {
        console.log(`📈 Position changed! ${contractsBefore} → ${contractsNow} contracts`);
      }
    }, 1000);

    await new Promise(resolve => setTimeout(resolve, 15000));
    clearInterval(checkInterval);
    console.log();

    // Get BTC position after order
    const btcPositionAfter = store.positions.find(p => p.symbol === 'BTC_USDT_Perp');
    const contractsAfter = btcPositionAfter?.contracts || 0;
    console.log('6. BTC position after order:', btcPositionAfter ? {
      side: btcPositionAfter.side,
      contracts: btcPositionAfter.contracts,
      entryPrice: btcPositionAfter.entryPrice,
      unrealizedPnl: btcPositionAfter.unrealizedPnl,
    } : 'None');
    console.log();

    // Summary
    console.log('=== Summary ===');
    console.log(`WebSocket messages received: ${wsMessages}`);
    console.log(`Contracts before: ${contractsBefore}`);
    console.log(`Contracts after: ${contractsAfter}`);
    console.log(`Position changed: ${contractsAfter !== contractsBefore ? 'YES ✅' : 'NO ⚠️'}`);
    console.log();

    if (contractsAfter !== contractsBefore) {
      console.log('✅ WebSocket position updates are working!');
    } else {
      console.log('⚠️  Position did not change (order might not have filled yet)');
    }

    // Clean up
    await exchange.dispose();
    process.exit(0);
  } catch (error) {
    console.error('❌ Error:', error);
    await exchange.dispose();
    process.exit(1);
  }
}

main();
