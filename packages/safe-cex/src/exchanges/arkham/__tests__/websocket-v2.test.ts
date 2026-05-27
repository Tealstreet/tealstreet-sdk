/**
 * Test script for Arkham WebSocket connections (v2 - trying unified endpoint)
 * Run with: tsx packages/safe-cex/src/exchanges/arkham/test-websocket-v2.ts
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

console.log('🔌 Testing Arkham WebSocket Connection (Unified Endpoint)\n');

/**
 * Test Unified WebSocket
 */
function testUnifiedWebSocket(): Promise<boolean> {
  return new Promise((resolve) => {
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Test: Unified WebSocket Connection');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    console.log('Connecting to:', WS_URL, '\n');

    const ws = new WebSocket(WS_URL);
    let messageCount = 0;
    const messagesReceived: any[] = [];
    let authenticated = false;

    const timeout = setTimeout(() => {
      console.log(`\n📊 Test Results:`);
      console.log(`Messages received: ${messageCount}`);
      console.log(`Authenticated: ${authenticated}`);
      if (messageCount > 0) {
        console.log(`Message types:`, [...new Set(messagesReceived.map((m) => m.type || m.channel))]);
      }
      ws.close();
      resolve(messageCount > 0);
    }, 15000); // Run for 15 seconds

    ws.on('open', () => {
      console.log('✅ WebSocket connected!\n');

      // Try subscribing to ticker first (public channel)
      console.log('📡 Subscribing to BTC_USDT ticker (public)...');
      ws.send(
        JSON.stringify({
          method: 'subscribe',
          params: {
            channel: 'ticker',
            symbol: 'BTC_USDT',
          },
        })
      );

      // Wait a bit then try authentication
      setTimeout(() => {
        console.log('\n🔐 Attempting authentication...');
        const timestamp = Date.now() * 1000;
        const expiry = timestamp + 5 * 1000000;
        const message = `${API_KEY}${expiry}websocket_auth`;

        const decodedSecret = Buffer.from(API_SECRET, 'base64');
        const hexSecret = decodedSecret.toString('hex');
        const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
          .update(message)
          .digest('base64');

        ws.send(
          JSON.stringify({
            method: 'authenticate',
            params: {
              apiKey: API_KEY,
              signature: signature,
              expires: expiry,
            },
          })
        );
      }, 2000);
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
          message.authenticated === true ||
          message.status === 'authenticated'
        ) {
          authenticated = true;
          console.log('✅ Authentication successful!\n');

          // Subscribe to private channels
          console.log('📡 Subscribing to orders channel...');
          ws.send(
            JSON.stringify({
              method: 'subscribe',
              params: {
                channel: 'orders',
              },
            })
          );

          console.log('📡 Subscribing to balances channel...\n');
          ws.send(
            JSON.stringify({
              method: 'subscribe',
              params: {
                channel: 'balances',
              },
            })
          );
        }
      } catch (err) {
        console.log('Error parsing message:', err);
        console.log('Raw data:', data.toString());
      }
    });

    ws.on('error', (error) => {
      console.log('❌ WebSocket error:', error.message);
      clearTimeout(timeout);
      resolve(false);
    });

    ws.on('close', (code, reason) => {
      console.log(`\nWebSocket closed (code: ${code}, reason: ${reason || 'none'})\n`);
      clearTimeout(timeout);
    });

    ws.on('ping', (data) => {
      console.log('📡 Received ping, sending pong...');
      ws.pong(data);
    });
  });
}

/**
 * Run test
 */
async function runTest() {
  console.log('🚀 Starting Arkham WebSocket Test (Unified Endpoint)\n');

  const result = await testUnifiedWebSocket();

  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('📊 Test Result');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  console.log(`${result ? '✅' : '❌'} Unified WebSocket Connection\n`);

  if (result) {
    console.log('🎉 WebSocket test passed!\n');
  } else {
    console.log('⚠️  WebSocket test failed.\n');
  }

  process.exit(0);
}

// Run test
runTest().catch((err) => {
  console.error('Test error:', err);
  process.exit(1);
});
