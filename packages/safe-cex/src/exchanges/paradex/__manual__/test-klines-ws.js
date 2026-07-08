const WebSocket = require('ws');

const WS_URL = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false';

console.log('\n🔍 Testing if Paradex supports OHLCV/klines WebSocket channel\n');

const ws = new WebSocket(WS_URL);
const messages = [];

const timeout = setTimeout(() => {
  console.log(`\n========== RESULTS ==========`);
  console.log(`Total messages received: ${messages.length}\n`);

  messages.forEach((msg, idx) => {
    console.log(`--- Message ${idx + 1} ---`);
    console.log(JSON.stringify(msg, null, 2));
    console.log('');
  });

  const errors = messages.filter(m => m.error);
  const successes = messages.filter(m => m.result && !m.error);

  console.log(`\n========== SUMMARY ==========`);
  console.log(`Successful subscriptions: ${successes.length}`);
  console.log(`Failed subscriptions: ${errors.length}`);

  if (successes.length === 0) {
    console.log('\n✅ CONFIRMED: Paradex does NOT support OHLCV/klines via WebSocket');
    console.log('   Use REST API endpoint: GET /markets/klines');
  } else {
    console.log('\n⚠️  UNEXPECTED: Some klines subscriptions succeeded!');
  }

  ws.close();
  process.exit(0);
}, 5000);

ws.on('open', () => {
  console.log('WebSocket connected\n');

  // Try various possible klines channel formats
  const testChannels = [
    'klines.BTC-USD-PERP',
    'klines.BTC-USD-PERP.1m',
    'candles.BTC-USD-PERP',
    'ohlcv.BTC-USD-PERP',
    'ohlcv.BTC-USD-PERP.1m',
  ];

  console.log('Testing the following channel formats:');
  testChannels.forEach((ch, idx) => {
    console.log(`  ${idx + 1}. ${ch}`);
  });
  console.log('\nSending subscription requests...\n');

  testChannels.forEach((channel, idx) => {
    const subscribeMsg = {
      id: idx + 1,
      jsonrpc: '2.0',
      method: 'subscribe',
      params: { channel },
    };
    ws.send(JSON.stringify(subscribeMsg));
  });
});

ws.on('message', (data) => {
  const msg = JSON.parse(data.toString());
  messages.push(msg);
});

ws.on('error', (error) => {
  console.error('WebSocket error:', error);
  clearTimeout(timeout);
  process.exit(1);
});
