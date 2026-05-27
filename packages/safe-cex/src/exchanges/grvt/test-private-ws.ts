#!/usr/bin/env tsx

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.log('⚠️ No credentials file found');
  process.exit(0);
}

async function testPrivateWebSocket() {
  const config = JSON.parse(readFileSync(credPath, 'utf-8'));

  console.log('\n=== GRVT Private WebSocket Test ===');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Trading Account ID:', config.grvtTradingAccountId);
  console.log('Signer Address:', config.grvtSignerAddress);
  console.log('==============================\n');

  const store = new DefaultStore();
  const exchange = new GrvtExchange(
    'grvt',
    {
      key: config.apiKey,
      secret: config.secret,
      testnet: config.testnet || false,
      subaccount: config.subaccountId,
      grvtSignerAddress: config.grvtSignerAddress,
      grvtTradingAccountId: config.grvtTradingAccountId,
    },
    store
  );

  try {
    console.log('1. Starting exchange (authenticates and connects private WebSocket)...');
    await exchange.start();
    console.log('✅ Exchange started');
    console.log('   Session cookie:', exchange.apiExtras?.sessionCookie ? 'Set' : 'Not set');
    console.log('   Account ID:', exchange.apiExtras?.accountId || 'Not set');
    console.log('   Private WS connected:', exchange.privateWebsocket.isConnected);
    console.log();

    // Set up fill event listener
    console.log('2. Setting up fill event listener...');
    let fillCount = 0;
    exchange.emitter.on('fill', (fill: any) => {
      console.log('💰 Fill Event:', {
        id: fill.id,
        symbol: fill.symbol,
        side: fill.side,
        price: fill.price,
        amount: fill.amount,
      });
      fillCount++;
    });
    console.log('✅ Fill listener set up\n');

    // Wait for WebSocket to fully connect and subscribe
    console.log('3. Waiting for WebSocket subscriptions (3 seconds)...');
    await new Promise((resolve) => setTimeout(resolve, 3000));
    console.log('✅ WebSocket should be subscribed\n');

    // Fetch initial orders and positions
    console.log('4. Fetching current orders and positions...');
    const initialOrders = await exchange.fetchOpenOrders();
    const initialPositions = await exchange.fetchPositions();
    console.log(`   Open orders: ${initialOrders.length}`);
    console.log(`   Positions: ${initialPositions.length}`);
    console.log();

    if (initialOrders.length > 0) {
      console.log('   Sample order:', {
        id: initialOrders[0].id,
        symbol: initialOrders[0].symbol,
        side: initialOrders[0].side,
        status: initialOrders[0].status,
        amount: initialOrders[0].amount,
        price: initialOrders[0].price,
      });
    }

    if (initialPositions.length > 0) {
      console.log('   Sample position:', {
        symbol: initialPositions[0].symbol,
        side: initialPositions[0].side,
        contracts: initialPositions[0].contracts,
        entryPrice: initialPositions[0].entryPrice,
        unrealizedPnl: initialPositions[0].unrealizedPnl,
      });
    }
    console.log();

    // Monitor store updates
    console.log('5. Monitoring store updates for 15 seconds...');
    console.log('   (Place or cancel orders in the GRVT UI to see updates)\n');

    const startOrders = store.orders.length;
    const startPositions = store.positions.length;

    await new Promise((resolve) => setTimeout(resolve, 15000));

    const endOrders = store.orders.length;
    const endPositions = store.positions.length;

    console.log('\n=== Summary ===');
    console.log(`Store Orders: ${startOrders} → ${endOrders} (${endOrders - startOrders >= 0 ? '+' : ''}${endOrders - startOrders})`);
    console.log(`Store Positions: ${startPositions} → ${endPositions} (${endPositions - startPositions >= 0 ? '+' : ''}${endPositions - startPositions})`);
    console.log(`Fill Events Received: ${fillCount}`);
    console.log();

    if (endOrders === startOrders && endPositions === startPositions && fillCount === 0) {
      console.log('⚠️ No WebSocket updates received.');
      console.log('This is expected if:');
      console.log('  1. You have no existing orders or positions');
      console.log('  2. No orders were placed/cancelled during the test');
      console.log();
      console.log('To test the private WebSocket:');
      console.log('  - Place an order in the GRVT UI while this script is running');
      console.log('  - Cancel an existing order');
      console.log('  - Or let an order fill to trigger fill events');
    } else {
      console.log('✅ Private WebSocket is working!');
      console.log('   Store is being updated with real-time order/position data');
    }
    console.log();

    console.log('✅ Test completed!\n');
  } catch (err: any) {
    console.error('\n❌ Error:', err.message);
    if (err.response?.data) {
      console.error('Response data:', JSON.stringify(err.response.data, null, 2));
    }
    if (err.stack) {
      console.error('Stack:', err.stack);
    }
  } finally {
    exchange.dispose();
  }
}

testPrivateWebSocket().catch(console.error);
