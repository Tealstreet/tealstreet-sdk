import { GrvtExchange } from './grvt.exchange';
import { DefaultStore } from '../../store/store.base';

const testOrderbook = async () => {
  const apiKey = process.env.GRVT_API_KEY || '';
  const apiSecret = process.env.GRVT_API_SECRET || '';
  const tradingAccountId = process.env.GRVT_TRADING_ACCOUNT_ID || '';

  if (!apiKey || !apiSecret) {
    console.error('Missing API credentials');
    process.exit(1);
  }

  const store = new DefaultStore();
  const exchange = new GrvtExchange('grvt', {
    key: apiKey,
    secret: apiSecret,
    testnet: true,
    grvtTradingAccountId: tradingAccountId,
  }, store);

  await exchange.start();
  console.log('Exchange started, subscribing to orderbook...');

  const unsubscribe = exchange.listenOrderBook('BTC_USDT_Perp', (orderbook) => {
    console.log('Orderbook update received:');
    console.log('  Bids:', orderbook.bids.slice(0, 3));
    console.log('  Asks:', orderbook.asks.slice(0, 3));
  });

  // Wait for some updates
  await new Promise((resolve) => setTimeout(resolve, 10000));

  unsubscribe();
  exchange.dispose();
  console.log('Test complete');
};

testOrderbook().catch(console.error);
