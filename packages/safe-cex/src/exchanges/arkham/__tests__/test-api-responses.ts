/**
 * Test Arkham API responses for balance, orders, and positions
 */
import { ArkhamExchange } from '../arkham.exchange';
import { DefaultStore } from '../../../store/store.base';
import fs from 'fs';
import path from 'path';

const credentialsPath = path.join(__dirname, '..', '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

async function testAPIResponses() {
  const store = new DefaultStore();
  const exchange = new ArkhamExchange(
    'arkham',
    {
      key: credentials.apiKey,
      secret: credentials.secret,
      testnet: false,
    },
    store
  );

  console.log('🧪 Testing Arkham Start Method and Store State\n');

  try {
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Calling exchange.start()...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    await exchange.start();

    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Waiting 5 seconds for WebSocket connections...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    await new Promise(resolve => setTimeout(resolve, 5000));

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Store State After start()');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log('Loaded flags:', JSON.stringify(store.loaded, null, 2));
    console.log('\nStore counts:');
    console.log(`  Markets: ${store.markets.length}`);
    console.log(`  Tickers: ${store.tickers.length}`);
    console.log(`  Positions: ${store.positions.length}`);
    console.log(`  Orders: ${store.orders.length}`);
    console.log(`\nBalance:`);
    console.log(`  Total: ${store.balance.total} USDT`);
    console.log(`  Free: ${store.balance.free}`);
    console.log(`  Used: ${store.balance.used}`);
    console.log(`  UPNL: ${store.balance.upnl}`);

    if (store.markets.length > 0) {
      console.log(`\nSample market (${store.markets[0].symbol}):`, JSON.stringify(store.markets[0], null, 2));
    }

    if (store.tickers.length > 0) {
      const btcTicker = store.tickers.find(t => t.symbol === 'BTC_USDT');
      if (btcTicker) {
        console.log(`\nBTC_USDT ticker:`, JSON.stringify(btcTicker, null, 2));
      }
    }

    if (store.positions.length > 0) {
      const btcPositions = store.positions.filter(p => p.symbol === 'BTC_USDT');
      console.log(`\nBTC_USDT positions (${btcPositions.length}):`, JSON.stringify(btcPositions, null, 2));

      const nonZeroPositions = store.positions.filter(p => p.contracts > 0 || p.notional > 0);
      console.log(`\nNon-zero positions: ${nonZeroPositions.length}`);
      if (nonZeroPositions.length > 0) {
        console.log(JSON.stringify(nonZeroPositions, null, 2));
      }
    }

    if (store.orders.length > 0) {
      console.log(`\nOrders:`, JSON.stringify(store.orders, null, 2));
    }

    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Summary');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    console.log(`${store.loaded.markets ? '✅' : '❌'} Markets loaded (${store.markets.length} items)`);
    console.log(`${store.loaded.tickers ? '✅' : '❌'} Tickers loaded (${store.tickers.length} items)`);
    console.log(`${store.loaded.positions ? '✅' : '❌'} Positions loaded (${store.positions.length} items)`);
    console.log(`${store.loaded.orders ? '✅' : '❌'} Orders loaded (${store.orders.length} items)`);
    console.log(`${store.loaded.balance ? '✅' : '❌'} Balance loaded (${store.balance.total} USDT)`);
    console.log();

  } catch (error: any) {
    console.error('❌ Error:', error.message);
    if (error.response) {
      console.error('Response:', JSON.stringify(error.response.data, null, 2));
    }
    throw error;
  } finally {
    exchange.dispose();
  }
}

testAPIResponses().catch(err => {
  console.error('Test failed:', err);
  process.exit(1);
});
