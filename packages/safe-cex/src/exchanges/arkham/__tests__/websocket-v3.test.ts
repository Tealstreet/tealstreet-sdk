/**
 * Test script for Arkham WebSocket with correct message format
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-websocket-v3.ts
 */

import WebSocket from 'ws';
import { createHmac } from 'crypto';
import fs from 'fs';
import path from 'path';

// Load API credentials
const credentialsPath = path.join(__dirname, '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;

const WS_URL = 'wss://arkm.com/ws';

console.log('= Testing Arkham WebSocket (v3 - Correct Format)\n');

function createSignature(apiKey: string, apiSecret: string): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000;
  const expiry = timestamp + 5 * 1000000;
  const method = 'GET';
  const path = '/ws';
  const body = '';

  const message = `${apiKey}${expiry}${method}${path}${body}`;

  const decodedSecret = Buffer.from(apiSecret, 'base64');
  const hexSecret = decodedSecret.toString('hex');
  const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
    .update(message)
    .digest('base64');

  return { signature, expiry };
}

async function testWebSocket() {
  console.log('');
  console.log('Test: WebSocket with Correct Message Format');
  console.log('\n');

  const { signature, expiry } = createSignature(API_KEY, API_SECRET);

  const ws = new WebSocket(WS_URL, {
    headers: {
      'Arkham-Api-Key': API_KEY,
      'Arkham-Signature': signature,
      'Arkham-Expires': expiry.toString(),
      'Arkham-Broker-Id': '1002',
    },
  });

  let messageCount = 0;

  ws.on('open', () => {
    console.log(' WebSocket connected!\n');

    // Subscribe to BTC_USDT ticker using correct format
    console.log('=á Subscribing to BTC_USDT ticker...');
    const tickerSub = {
      method: 'subscribe',
      args: {
        channel: 'ticker',
        params: {
          symbol: 'BTC_USDT',
        },
      },
      confirmationId: 'ticker-sub-1',
    };
    ws.send(JSON.stringify(tickerSub));

    // Subscribe to orderbook
    setTimeout(() => {
      console.log('=á Subscribing to BTC_USDT L2 orderbook...\n');
      const bookSub = {
        method: 'subscribe',
        args: {
          channel: 'l2_book',
          params: {
            symbol: 'BTC_USDT',
          },
        },
        confirmationId: 'book-sub-1',
      };
      ws.send(JSON.stringify(bookSub));
    }, 1000);

    // Subscribe to private channel (order_statuses)
    setTimeout(() => {
      console.log('=á Subscribing to order_statuses (private channel)...\n');
      const ordersSub = {
        method: 'subscribe',
        args: {
          channel: 'order_statuses',
        },
        confirmationId: 'orders-sub-1',
      };
      ws.send(JSON.stringify(ordersSub));
    }, 2000);
  });

  ws.on('message', (data: Buffer) => {
    try {
      const message = JSON.parse(data.toString());
      messageCount++;

      console.log(`=è Message ${messageCount}:`);

      if (message.error) {
        console.log(`L Error: ${message.error.name} - ${message.error.message}`);
      } else if (message.confirmationId) {
        console.log(` Confirmation for: ${message.confirmationId}`);
      } else if (message.channel) {
        console.log(`=Ê Channel: ${message.channel}`);
        if (message.data) {
          const keys = Object.keys(message.data);
          console.log(`   Data keys: ${keys.join(', ')}`);
        }
      } else {
        console.log(`=Ë Message:`, JSON.stringify(message, null, 2));
      }
      console.log();
    } catch (err) {
      console.log('Error parsing message:', err);
      console.log('Raw data:', data.toString());
    }
  });

  ws.on('error', (error) => {
    console.log('L WebSocket error:', error.message);
  });

  ws.on('close', (code, reason) => {
    console.log(`\n`);
    console.log(`WebSocket closed (code: ${code}, reason: ${reason || 'none'})`);
    console.log(`Total messages received: ${messageCount}`);
    console.log('\n');
  });

  // Run for 15 seconds
  setTimeout(() => {
    console.log('ñ  Test timeout - closing connection...\n');
    ws.close();
  }, 15000);
}

testWebSocket().catch((err) => {
  console.error('Test error:', err);
  process.exit(1);
});
