const WebSocket = require('ws');

const WS_URL = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false';
const TEST_MARKET = 'BTC-USD-PERP';

const ws = new WebSocket(WS_URL);
const messages = [];

const timeout = setTimeout(() => {
  console.log(`\n\n========== RECEIVED ${messages.length} MESSAGES ==========`);
  messages.forEach((msg, idx) => {
    console.log(`\n--- Message ${idx + 1} ---`);
    console.log(JSON.stringify(msg, null, 2));
  });
  ws.close();
  process.exit(0);
}, 10000);

ws.on('open', () => {
  console.log('WebSocket connected');

  // Try subscribing to order_book with correct format
  const channel = `order_book.${TEST_MARKET}.snapshot@15@100ms@0_1`;
  const subscribeMsg = {
    jsonrpc: '2.0',
    method: 'subscribe',
    params: { channel },
    id: 1,
  };

  console.log('\n========== SENDING SUBSCRIPTION ==========');
  console.log(JSON.stringify(subscribeMsg, null, 2));
  ws.send(JSON.stringify(subscribeMsg));
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

ws.on('close', () => {
  console.log('WebSocket closed');
});
