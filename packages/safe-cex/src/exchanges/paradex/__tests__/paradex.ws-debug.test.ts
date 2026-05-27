import { describe, it } from 'vitest';
import WebSocket from 'ws';

/**
 * Debug test to see exactly what messages Paradex WebSocket sends
 */
describe('Paradex WebSocket Debug', () => {
  const WS_URL = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false';
  const TEST_MARKET = 'BTC-USD-PERP';

  it.only('should log all messages received for 10 seconds', async () => {
    const ws = new WebSocket(WS_URL);
    const messages: any[] = [];

    await new Promise<void>((resolve) => {
      const timeout = setTimeout(() => {
        console.log(`\n\n========== RECEIVED ${messages.length} MESSAGES ==========`);
        messages.forEach((msg, idx) => {
          console.log(`\n--- Message ${idx + 1} ---`);
          console.log(JSON.stringify(msg, null, 2));
        });
        ws.close();
        resolve();
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

      ws.on('message', (data: WebSocket.Data) => {
        const msg = JSON.parse(data.toString());
        messages.push(msg);
      });

      ws.on('error', (error) => {
        console.error('WebSocket error:', error);
        clearTimeout(timeout);
        resolve();
      });

      ws.on('close', () => {
        console.log('WebSocket closed');
      });
    });
  }, 15000);
});
