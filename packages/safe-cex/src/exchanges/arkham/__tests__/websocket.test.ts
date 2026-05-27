/**
 * Test script for Arkham WebSocket connections
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-websocket.ts
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

const PUBLIC_WS_URL = 'wss://arkm.com/ws/public';
const PRIVATE_WS_URL = 'wss://arkm.com/ws/private';

console.log('🔌 Testing Arkham WebSocket Connections\n');

/**
 * Test Public WebSocket
 */
function testPublicWebSocket(): Promise<boolean> {
  return new Promise((resolve) => {
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Test 1: Public WebSocket Connection');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    const ws = new WebSocket(PUBLIC_WS_URL);
    let messageCount = 0;
    const messagesReceived: any[] = [];

    const timeout = setTimeout(() => {
      console.log(`\n✅ Public WebSocket test complete!`);
      console.log(`Messages received: ${messageCount}`);
      console.log(`Message types:`, [...new Set(messagesReceived.map((m) => m.type || m.channel))]);
      ws.close();
      resolve(true);
    }, 10000); // Run for 10 seconds

    ws.on('open', () => {
      console.log('✅ Public WebSocket connected!\n');

      // Subscribe to ticker for BTC_USDT
      console.log('📡 Subscribing to BTC_USDT ticker...');
      ws.send(
        JSON.stringify({
          type: 'subscribe',
          channel: 'ticker',
          symbol: 'BTC_USDT',
        })
      );

      // Subscribe to orderbook for BTC_USDT
      console.log('📡 Subscribing to BTC_USDT orderbook...');
      ws.send(
        JSON.stringify({
          type: 'subscribe',
          channel: 'book',
          symbol: 'BTC_USDT',
        })
      );

      // Subscribe to trades for BTC_USDT
      console.log('📡 Subscribing to BTC_USDT trades...\n');
      ws.send(
        JSON.stringify({
          type: 'subscribe',
          channel: 'trades',
          symbol: 'BTC_USDT',
        })
      );
    });

    ws.on('message', (data: WebSocket.Data) => {
      try {
        const message = JSON.parse(data.toString());
        messageCount++;
        messagesReceived.push(message);

        // Log first few messages
        if (messageCount <= 5) {
          console.log(`📨 Message ${messageCount}:`, JSON.stringify(message, null, 2), '\n');
        } else if (messageCount === 6) {
          console.log('... (suppressing further messages)\n');
        }
      } catch (err) {
        console.log('Error parsing message:', err);
      }
    });

    ws.on('error', (error) => {
      console.log('❌ WebSocket error:', error.message);
      clearTimeout(timeout);
      resolve(false);
    });

    ws.on('close', () => {
      console.log('Public WebSocket closed\n');
      clearTimeout(timeout);
    });
  });
}

/**
 * Test Private WebSocket
 */
function testPrivateWebSocket(): Promise<boolean> {
  return new Promise((resolve) => {
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Test 2: Private WebSocket Connection & Auth');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    const ws = new WebSocket(PRIVATE_WS_URL);
    let authenticated = false;
    let messageCount = 0;
    const messagesReceived: any[] = [];

    const timeout = setTimeout(() => {
      if (authenticated) {
        console.log(`\n✅ Private WebSocket test complete!`);
        console.log(`Authenticated: ${authenticated}`);
        console.log(`Messages received: ${messageCount}`);
        console.log(`Message types:`, [...new Set(messagesReceived.map((m) => m.type || m.channel))]);
      }
      ws.close();
      resolve(authenticated);
    }, 10000); // Run for 10 seconds

    ws.on('open', () => {
      console.log('✅ Private WebSocket connected!\n');

      // Authenticate
      const timestamp = Date.now() * 1000;
      const expiry = timestamp + 5 * 1000000;
      const message = `${API_KEY}${expiry}websocket_auth`;

      const decodedSecret = Buffer.from(API_SECRET, 'base64');
      const hexSecret = decodedSecret.toString('hex');
      const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
        .update(message)
        .digest('base64');

      console.log('🔐 Sending authentication...');
      ws.send(
        JSON.stringify({
          type: 'auth',
          apiKey: API_KEY,
          signature: signature,
          expires: expiry,
        })
      );
    });

    ws.on('message', (data: WebSocket.Data) => {
      try {
        const message = JSON.parse(data.toString());
        messageCount++;
        messagesReceived.push(message);

        console.log(`📨 Message ${messageCount}:`, JSON.stringify(message, null, 2), '\n');

        // Check if authenticated
        if (
          message.type === 'auth' ||
          message.type === 'authenticated' ||
          message.authenticated === true
        ) {
          authenticated = true;
          console.log('✅ Authentication successful!\n');

          // Subscribe to private channels
          console.log('📡 Subscribing to orders channel...');
          ws.send(
            JSON.stringify({
              type: 'subscribe',
              channel: 'orders',
            })
          );

          console.log('📡 Subscribing to positions channel...');
          ws.send(
            JSON.stringify({
              type: 'subscribe',
              channel: 'positions',
            })
          );

          console.log('📡 Subscribing to balances channel...\n');
          ws.send(
            JSON.stringify({
              type: 'subscribe',
              channel: 'balances',
            })
          );
        }
      } catch (err) {
        console.log('Error parsing message:', err);
      }
    });

    ws.on('error', (error) => {
      console.log('❌ WebSocket error:', error.message);
      clearTimeout(timeout);
      resolve(false);
    });

    ws.on('close', () => {
      console.log('Private WebSocket closed\n');
      clearTimeout(timeout);
    });
  });
}

/**
 * Run all WebSocket tests
 */
async function runAllTests() {
  console.log('🚀 Starting Arkham WebSocket Tests\n');

  const publicResult = await testPublicWebSocket();
  const privateResult = await testPrivateWebSocket();

  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📊 WebSocket Test Results Summary');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  console.log(`${publicResult ? '✅' : '❌'} Public WebSocket`);
  console.log(`${privateResult ? '✅' : '❌'} Private WebSocket (Auth + Subscriptions)`);

  if (publicResult && privateResult) {
    console.log('\n🎉 All WebSocket tests passed!\n');
  } else {
    console.log('\n⚠️  Some WebSocket tests failed.\n');
  }

  process.exit(0);
}

// Run tests
runAllTests().catch((err) => {
  console.error('Test error:', err);
  process.exit(1);
});
