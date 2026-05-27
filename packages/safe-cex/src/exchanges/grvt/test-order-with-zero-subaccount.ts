/**
 * Test order placement with sub_account_id = '0'
 *
 * Run with: npx tsx src/exchanges/grvt/test-order-with-zero-subaccount.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';

const credPath = path.resolve(__dirname, '.apikey.json');

async function main() {
  if (!fs.existsSync(credPath)) {
    console.log('⚠️ No credentials file found');
    process.exit(0);
  }

  const config = JSON.parse(fs.readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Order Test with sub_account_id = "0" ===\n');
  console.log('Config:');
  console.log('  - API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('  - grvtTradingAccountId:', config.grvtTradingAccountId || '(not set, will default to "0")');
  console.log('  - Secret:', config.apiSecret?.substring(0, 10) + '...');

  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: config.apiKey,
      secret: config.apiSecret,
      testnet: config.testnet || false,
      grvtSignerAddress: config.grvtSignerAddress,
      grvtTradingAccountId: config.grvtTradingAccountId, // Will be undefined, should default to '0'
    },
    store
  );

  try {
    console.log('\n1. Starting exchange...');
    await exchange.start();
    console.log('✅ Exchange started\n');

    console.log('2. Current grvtTradingAccountId:', (exchange as any).grvtTradingAccountId || '(not set)');

    // Find BTC market
    const btcMarket = store.markets.find((m) => m.symbol.includes('BTC'));
    if (!btcMarket) {
      console.log('❌ No BTC market found');
      return;
    }
    console.log('\n3. Using market:', btcMarket.symbol);

    // Try to place a small limit order far from market price (won't fill)
    const ticker = store.tickers.find((t) => t.symbol === btcMarket.symbol);
    const currentPrice = ticker?.last || 100000;
    const orderPrice = Math.round(currentPrice * 0.5); // 50% below market - won't fill

    console.log('\n4. Attempting to place order:');
    console.log('  - Symbol:', btcMarket.symbol);
    console.log('  - Side: BUY');
    console.log('  - Type: LIMIT');
    console.log('  - Amount:', btcMarket.limits.amount.min);
    console.log('  - Price:', orderPrice, '(50% below market to avoid fill)');

    try {
      const orderIds = await exchange.placeOrder({
        symbol: btcMarket.symbol,
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: btcMarket.limits.amount.min,
        price: orderPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      console.log('\n✅ Order placed successfully!');
      console.log('  - Order IDs:', orderIds);

      // Cancel the order immediately
      if (orderIds.length > 0) {
        console.log('\n5. Cancelling order...');
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
        console.log('✅ Order cancelled');
      }
    } catch (orderErr: any) {
      console.log('\n❌ Order failed:', orderErr.message);
      if (orderErr.response?.data) {
        console.log('  - API Response:', JSON.stringify(orderErr.response.data, null, 2));
      }
    }
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response?.data) {
      console.log('  - API Response:', JSON.stringify(err.response.data, null, 2));
    }
  } finally {
    exchange.dispose();
  }

  console.log('\n=== Test Complete ===');
}

main().catch(console.error);
