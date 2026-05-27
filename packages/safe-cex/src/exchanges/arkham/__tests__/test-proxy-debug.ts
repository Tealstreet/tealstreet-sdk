import crypto from 'crypto';
import fs from 'fs';
import path from 'path';

import WebSocket from 'ws';

// Load API credentials
const apiKeyPath = path.join(__dirname, '../.apikey.json');
const { apiKey, secret } = JSON.parse(fs.readFileSync(apiKeyPath, 'utf-8'));

console.log('🔌 Testing Arkham WebSocket Proxy Connection');
console.log('━'.repeat(60));

// Generate auth parameters
const timestamp = Date.now() * 1000; // Convert to microseconds
const expiry = timestamp + 5 * 1000000; // 5 seconds in the future (in microseconds)
const method = 'GET';
const requestPath = '/ws';
const message = `${apiKey}${expiry}${method}${requestPath}`;
const decodedSecret = Buffer.from(secret, 'base64');
const signature = crypto.createHmac('sha256', decodedSecret).update(message).digest('base64');

const authMessage = {
  type: 'auth',
  apikey: apiKey,
  expires: expiry.toString(),
  signature,
};

console.log('📝 Auth message:', JSON.stringify(authMessage, null, 2));
console.log('');

// Connect to proxy
const ws = new WebSocket('wss://fsjnpy143pbz.jp.tealstreet.io/arkm');

ws.on('open', () => {
  console.log('✅ Connected to proxy');
  console.log('📤 Sending auth message...');
  ws.send(JSON.stringify(authMessage));
});

ws.on('message', (data: Buffer) => {
  const message = data.toString();
  console.log('📥 Received:', message);

  try {
    const parsed = JSON.parse(message);

    if (parsed.type === 'auth_success') {
      console.log('✅ Authentication successful!');
      console.log('');
      console.log('📤 Subscribing to BTC_USDT ticker...');
      ws.send(
        JSON.stringify({
          method: 'subscribe',
          args: {
            channel: 'ticker',
            params: {
              symbol: 'BTC_USDT',
            },
          },
        }),
      );

      console.log('📤 Subscribing to BTC_USDT l2_updates...');
      ws.send(
        JSON.stringify({
          method: 'subscribe',
          args: {
            channel: 'l2_updates',
            params: {
              symbol: 'BTC_USDT',
            },
          },
        }),
      );
    } else if (parsed.type === 'error') {
      console.error('❌ Proxy error:', parsed.error);
    } else if (parsed.channel) {
      console.log(`📊 ${parsed.channel} update for ${parsed.data?.symbol || 'unknown'}`);
    }
  } catch (err) {
    console.error('Failed to parse message:', err);
  }
});

ws.on('error', (error) => {
  console.error('❌ WebSocket error:', error);
});

ws.on('close', (code, reason) => {
  console.log(`🔌 Connection closed: ${code} - ${reason}`);
});

// Keep alive for 30 seconds
setTimeout(() => {
  console.log('⏰ Test timeout, closing connection...');
  ws.close();
  process.exit(0);
}, 30000);
