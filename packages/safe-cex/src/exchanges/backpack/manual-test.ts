/**
 * Manual Test Script for Backpack Exchange
 *
 * Run with: npx tsx packages/safe-cex/src/exchanges/backpack/manual-test.ts
 */

import { BackpackExchange } from './backpack.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce } from '../../types';
import * as fs from 'fs';
import * as path from 'path';

async function main() {
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📝 Backpack Manual Test');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  // Load credentials
  const credsPath = path.join(__dirname, '.apikey.json');
  if (!fs.existsSync(credsPath)) {
    console.error('❌ .apikey.json not found');
    process.exit(1);
  }

  const testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
  console.log('✓ Loaded credentials from .apikey.json');
  console.log(`  API Key: ${testCreds.apiKey.substring(0, 20)}...`);
  console.log(`  Testnet: ${testCreds.testnet}\n`);

  // Create store and exchange
  const store = new DefaultStore();
  const exchange = new BackpackExchange(
    'backpack',
    {
      key: testCreds.apiKey,
      secret: testCreds.secret,
      testnet: testCreds.testnet || false,
    },
    store
  );

  // Listen for errors
  exchange.on('error', (error: string) => {
    console.log('❌ Error event:', error);
  });

  try {
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('1. Testing Market Data');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Fetch markets
    console.log('🔄 Fetching markets...');
    const markets = await exchange.fetchMarkets();
    console.log(`✓ Fetched ${markets.length} markets`);

    if (markets.length > 0) {
      console.log(`  Sample markets:` );
      markets.slice(0, 5).forEach(m => {
        console.log(`    - ${m.symbol} (${m.id})`);
      });
    }
    console.log();

    // Store markets
    store.update({ markets });

    // Fetch tickers
    console.log('🔄 Fetching tickers...');
    const tickers = await exchange.fetchTickers();
    console.log(`✓ Fetched ${tickers.length} tickers`);

    if (tickers.length > 0) {
      const sampleTicker = tickers[0];
      console.log(`  Sample ticker (${sampleTicker.symbol}):`);
      console.log(`    Bid: ${sampleTicker.bid}, Ask: ${sampleTicker.ask}`);
      console.log(`    Last: ${sampleTicker.last}, Volume: ${sampleTicker.volume}`);
    }
    console.log();

    // Store tickers
    store.update({ tickers });

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('2. Testing Account Data');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Fetch balance
    console.log('🔄 Fetching balance...');
    const balance = await exchange.fetchBalance();
    console.log(`✓ Account balance:`);
    console.log(`    Total: ${balance.total}`);
    console.log(`    Free: ${balance.free}`);
    console.log(`    Used: ${balance.used}`);
    console.log();

    // Fetch positions
    console.log('🔄 Fetching positions...');
    const positions = await exchange.fetchPositions();
    console.log(`✓ Fetched ${positions.length} positions`);

    const actualPositions = positions.filter(p => p.contracts > 0);
    const fakePositions = positions.filter(p => p.contracts === 0);
    console.log(`    Actual positions: ${actualPositions.length}`);
    console.log(`    Fake positions: ${fakePositions.length}`);

    if (actualPositions.length > 0) {
      console.log(`\n  Actual positions:`);
      actualPositions.forEach(p => {
        console.log(`    - ${p.symbol} ${p.side}: ${p.contracts} contracts @ ${p.entryPrice}`);
        console.log(`      PnL: ${p.unrealizedPnl}`);
      });
    }
    console.log();

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('3. Testing WebSocket Connections');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Start WebSockets
    console.log('🔄 Starting WebSocket connections...');
    await exchange.start();
    console.log('✓ WebSocket connections started');
    console.log();

    // Wait for some updates
    console.log('⏳ Waiting 5 seconds for WebSocket updates...');
    await new Promise(resolve => setTimeout(resolve, 5000));

    const updatedTicker = store.tickers.find(t => t.symbol === markets[0].symbol);
    if (updatedTicker) {
      console.log(`✓ Received ticker update for ${updatedTicker.symbol}:`);
      console.log(`    Bid: ${updatedTicker.bid}, Ask: ${updatedTicker.ask}`);
      console.log(`    Last: ${updatedTicker.last}`);
    }
    console.log();

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('4. Testing Order Operations');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Find a suitable market (SOL/USDC or first available)
    const testMarket = markets.find(m =>
      m.symbol === 'SOL/USDC' || m.symbol === 'SOL_USDC'
    ) || markets[0];

    console.log(`Using market: ${testMarket.symbol}`);

    const testTicker = tickers.find(t => t.symbol === testMarket.symbol);
    if (!testTicker) {
      console.error('❌ No ticker found for test market');
      process.exit(1);
    }

    console.log(`  Current bid: ${testTicker.bid}`);
    console.log(`  Current ask: ${testTicker.ask}`);
    console.log();

    // Place a limit order far below market price (won't fill immediately)
    const orderPrice = testTicker.bid * 0.5; // 50% below current bid
    const orderAmount = 0.01; // Very small amount

    console.log(`🔄 Placing limit buy order:`);
    console.log(`    Symbol: ${testMarket.symbol}`);
    console.log(`    Side: Buy`);
    console.log(`    Amount: ${orderAmount}`);
    console.log(`    Price: ${orderPrice.toFixed(2)} (50% below market)`);

    let orderId: string | undefined;
    try {
      const orderIds = await exchange.placeOrder({
        symbol: testMarket.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderAmount,
        price: orderPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      orderId = orderIds[0];
      console.log(`✓ Order placed successfully: ${orderId}`);
      console.log();

      // Wait for order to appear in system
      console.log('⏳ Waiting 2 seconds for order to be registered...');
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Fetch orders to verify
      console.log('🔄 Fetching open orders...');
      const orders = await exchange.fetchOrders();
      console.log(`✓ Found ${orders.length} open orders`);

      const placedOrder = orders.find(o => o.id === orderId);
      if (placedOrder) {
        console.log(`  ✓ Order ${orderId} confirmed:`);
        console.log(`      Symbol: ${placedOrder.symbol}`);
        console.log(`      Side: ${placedOrder.side}`);
        console.log(`      Type: ${placedOrder.type}`);
        console.log(`      Price: ${placedOrder.price}`);
        console.log(`      Amount: ${placedOrder.amount}`);
        console.log(`      Status: ${placedOrder.status}`);
      } else {
        console.log(`  ⚠️  Order not found in open orders (may have been processed)`);
      }
      console.log();

      // Cancel the order
      if (placedOrder) {
        console.log(`🔄 Canceling order ${orderId}...`);
        await exchange.cancelOrder(placedOrder);
        console.log(`✓ Order canceled successfully`);
        console.log();

        // Verify cancellation
        console.log('⏳ Waiting 2 seconds for cancellation to propagate...');
        await new Promise(resolve => setTimeout(resolve, 2000));

        const ordersAfterCancel = await exchange.fetchOrders();
        const canceledOrder = ordersAfterCancel.find(o => o.id === orderId);

        if (!canceledOrder) {
          console.log(`✓ Order ${orderId} no longer in open orders (canceled)`);
        } else {
          console.log(`  Order status: ${canceledOrder.status}`);
        }
      }
    } catch (error: any) {
      console.error('❌ Error during order operations:');
      console.error(error.response?.data || error.message || error);
    }
    console.log();

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('✓ All tests completed successfully!');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');

  } catch (error: any) {
    console.error('\n❌ Test failed:');
    console.error(error.response?.data || error.message || error);
    if (error.stack) {
      console.error('\nStack trace:');
      console.error(error.stack);
    }
    process.exit(1);
  } finally {
    // Clean up WebSocket connections
    if (exchange.publicWebsocket) {
      exchange.publicWebsocket.dispose();
    }
    if (exchange.privateWebsocket) {
      exchange.privateWebsocket.dispose();
    }

    // Give time for cleanup
    await new Promise(resolve => setTimeout(resolve, 1000));
    process.exit(0);
  }
}

main();
