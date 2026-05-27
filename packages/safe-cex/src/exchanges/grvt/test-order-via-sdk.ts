/**
 * Test order placement using GRVT SDK
 *
 * Run with: npx tsx src/exchanges/grvt/test-order-via-sdk.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';

const credPath = path.resolve(__dirname, '.apikey.json');

async function main() {
  console.log('=== GRVT Order Test via SDK ===\n');

  if (!fs.existsSync(credPath)) {
    console.log('⚠️ No credentials file found');
    process.exit(0);
  }

  const config = JSON.parse(fs.readFileSync(credPath, 'utf-8'));

  console.log('Config:');
  console.log('  API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('  Testnet:', config.testnet ?? false);
  console.log('  grvtTradingAccountId:', config.grvtTradingAccountId || '(not set)');
  console.log('  grvtSignerAddress:', config.grvtSignerAddress || '(not set)');

  const store = new DefaultStore();

  // Test with different grvtTradingAccountId values
  const testCases = [
    { id: undefined, desc: 'undefined (will default to "0")' },
    { id: '0', desc: '"0"' },
    { id: '', desc: 'empty string' },
  ];

  for (const testCase of testCases) {
    console.log(`\n${'='.repeat(60)}`);
    console.log(`Testing with grvtTradingAccountId = ${testCase.desc}`);
    console.log('='.repeat(60));

    const exchange = new GrvtExchange(
      'grvt',
      {
        key: config.apiKey,
        secret: config.apiSecret,
        testnet: config.testnet || false,
        grvtSignerAddress: config.grvtSignerAddress,
        grvtTradingAccountId: testCase.id,
      },
      store
    );

    try {
      console.log('\n1. Starting exchange...');
      await exchange.start();
      console.log('   ✅ Exchange started');

      // Check what grvtTradingAccountId was set to
      console.log('   grvtTradingAccountId after start:', (exchange as any).grvtTradingAccountId);

      // Find BTC market
      const btcMarket = store.markets.find((m) => m.symbol.includes('BTC') && m.symbol.includes('PERP'));
      if (!btcMarket) {
        console.log('   ❌ No BTC market found');
        exchange.dispose();
        continue;
      }
      console.log('   Using market:', btcMarket.symbol);

      // Get current price
      const ticker = store.tickers.find((t) => t.symbol === btcMarket.symbol);
      const currentPrice = ticker?.last || 100000;
      const orderPrice = Math.round(currentPrice * 0.5); // 50% below to avoid fill

      console.log('\n2. Placing order...');
      console.log('   Symbol:', btcMarket.symbol);
      console.log('   Side: BUY');
      console.log('   Type: LIMIT');
      console.log('   Amount:', btcMarket.limits.amount.min);
      console.log('   Price:', orderPrice);

      try {
        const orderIds = await exchange.placeOrder({
          symbol: btcMarket.symbol,
          side: OrderSide.Buy,
          type: OrderType.Limit,
          amount: btcMarket.limits.amount.min,
          price: orderPrice,
          timeInForce: OrderTimeInForce.GoodTillCancel,
        });

        console.log('\n   ✅ Order placed successfully!');
        console.log('   Order IDs:', orderIds);

        // Try to cancel
        if (orderIds.length > 0) {
          console.log('\n3. Cancelling order...');
          await exchange.cancelOrders([{
            id: orderIds[0],
            symbol: btcMarket.symbol,
            status: 'open' as any,
            type: OrderType.Limit,
            side: OrderSide.Buy,
            price: orderPrice,
            amount: btcMarket.limits.amount.min,
            filled: 0,
            remaining: btcMarket.limits.amount.min,
            reduceOnly: false,
            lastUpdate: Date.now(),
          }]);
          console.log('   ✅ Order cancelled');
        }

        // If we got here, we found the working configuration!
        console.log('\n🎉 SUCCESS! Working configuration found!');
        console.log('   grvtTradingAccountId:', testCase.id);
        exchange.dispose();
        return;
      } catch (orderErr: any) {
        console.log('\n   ❌ Order failed:', orderErr.message);
        if (orderErr.response?.data) {
          console.log('   API Response:', JSON.stringify(orderErr.response.data, null, 2));
        }
      }
    } catch (err: any) {
      console.log('\n   ❌ Error:', err.message);
    } finally {
      exchange.dispose();
    }
  }

  console.log('\n=== All test cases completed ===');
}

main().catch(console.error);
